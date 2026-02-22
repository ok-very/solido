// Organism SDF render pipeline for egui-wgpu.
//
// Renders L-shaped organisms as fullscreen-triangle SDF passes inside an
// egui::CentralPanel via PaintCallback. The shader uses direct boolean SDF
// composition ("quantized then smoothed"): bounding box minus scoop cutout,
// with per-corner rounding for crisp LCARS-style panels.
// Text labels are rendered via an SDF font atlas sampled in the fragment shader.
//
// SDF references: Inigo Quilez, https://iquilezles.org/articles/distfunctions2d/

use bytemuck::{Pod, Zeroable};
use std::mem;
use wgpu::util::DeviceExt;

use super::font_atlas::FontAtlas;
use super::shape_atlas::ShapeAtlas;
use crate::recorder::CapturedFrame;

// ============================================================================
// GPU-side data structures (must match organism.wgsl exactly)
// ============================================================================

/// Uniform buffer layout — 32 bytes, two 16-byte rows.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub viewport: [f32; 2],
    pub time: f32,
    pub organism_count: f32,
    pub dpr: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Per-organism data — 48 bytes, three 16-byte rows.
/// The last two fields carry glyph range indices into the TextGlyph buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct OrganismGpuData {
    pub pos: [f32; 2],
    pub stem_size: [f32; 2],
    pub bar_offset: [f32; 2],
    pub bar_size: [f32; 2],
    pub corner_radius: f32,
    pub fillet_radius: f32,
    pub glyph_start: u32,
    pub glyph_count: u32,
}

/// Per-glyph instance data — 32 bytes, two 16-byte rows.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TextGlyphGpu {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv_rect: [f32; 4],
}

// Compile-time layout assertions
const _: () = assert!(mem::size_of::<Uniforms>() == 32);
const _: () = assert!(mem::size_of::<OrganismGpuData>() == 48);
const _: () = assert!(mem::size_of::<TextGlyphGpu>() == 32);

// ============================================================================
// Persistent GPU resources (stored in CallbackResources)
// ============================================================================

pub struct OrganismRenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    organism_buffer: wgpu::Buffer,
    glyph_buffer: wgpu::Buffer,
    font_atlas_view: wgpu::TextureView,
    font_sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    organism_capacity: usize,
    glyph_capacity: usize,
    // Shape atlas resources
    shape_atlas_view: wgpu::TextureView,
    shape_sampler: wgpu::Sampler,
    // Capture pipeline resources
    capture_pipeline: wgpu::RenderPipeline,
    capture_texture: Option<wgpu::Texture>,
    capture_texture_view: Option<wgpu::TextureView>,
    staging_buffer: Option<wgpu::Buffer>,
    capture_width: u32,
    capture_height: u32,
}

// ============================================================================
// Initialization
// ============================================================================

const MIN_STORAGE_BYTES: u64 = 48;
const INITIAL_ORGANISM_CAPACITY: usize = 16;
const INITIAL_GLYPH_CAPACITY: usize = 128;

pub fn init_resources(render_state: &egui_wgpu::RenderState, atlas: &FontAtlas, shape_atlas: &ShapeAtlas) {
    let device = &render_state.device;
    let queue = &render_state.queue;

    let shader = device.create_shader_module(wgpu::include_wgsl!("organism.wgsl"));

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("solido_organism_bgl"),
        entries: &[
            // binding 0: Uniforms
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        mem::size_of::<Uniforms>() as u64,
                    ),
                },
                count: None,
            },
            // binding 1: OrganismData[]
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        mem::size_of::<OrganismGpuData>() as u64,
                    ),
                },
                count: None,
            },
            // binding 2: SDF font atlas texture
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // binding 3: sampler for atlas
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // binding 4: TextGlyph[]
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        mem::size_of::<TextGlyphGpu>() as u64,
                    ),
                },
                count: None,
            },
            // binding 5: shape atlas texture (MTSDF corner elements)
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // binding 6: sampler for shape atlas
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("solido_organism_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solido_organism_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_state.target_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    });

    // -- Capture pipeline (transparent background, Rgba8Unorm) ---------------
    let capture_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solido_capture_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_capture"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    });

    // -- Buffers -------------------------------------------------------------
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("solido_uniform_buf"),
        contents: bytemuck::bytes_of(&Uniforms::zeroed()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let organism_buf_size =
        (mem::size_of::<OrganismGpuData>() * INITIAL_ORGANISM_CAPACITY) as u64;
    let organism_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("solido_organism_buf"),
        size: organism_buf_size.max(MIN_STORAGE_BYTES),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let glyph_buf_size =
        (mem::size_of::<TextGlyphGpu>() * INITIAL_GLYPH_CAPACITY) as u64;
    let glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("solido_glyph_buf"),
        size: glyph_buf_size.max(MIN_STORAGE_BYTES),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // -- Font atlas texture --------------------------------------------------
    let (font_atlas_view, font_sampler) = create_font_atlas_texture(device, queue, atlas);

    // -- Shape atlas texture -------------------------------------------------
    let (shape_atlas_view, shape_sampler) = create_shape_atlas_texture(device, queue, shape_atlas);

    // -- Bind group ----------------------------------------------------------
    let bind_group = create_bind_group(
        device,
        &bind_group_layout,
        &uniform_buffer,
        &organism_buffer,
        &font_atlas_view,
        &font_sampler,
        &glyph_buffer,
        &shape_atlas_view,
        &shape_sampler,
    );

    let resources = OrganismRenderResources {
        pipeline,
        bind_group_layout,
        uniform_buffer,
        organism_buffer,
        glyph_buffer,
        font_atlas_view,
        font_sampler,
        bind_group,
        organism_capacity: INITIAL_ORGANISM_CAPACITY,
        glyph_capacity: INITIAL_GLYPH_CAPACITY,
        shape_atlas_view,
        shape_sampler,
        capture_pipeline,
        capture_texture: None,
        capture_texture_view: None,
        staging_buffer: None,
        capture_width: 0,
        capture_height: 0,
    };

    render_state
        .renderer
        .write()
        .callback_resources
        .insert(resources);
}

fn create_font_atlas_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &FontAtlas,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solido_font_atlas"),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width * 4), // 4 bytes per Rgba8Unorm texel
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("solido_font_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });

    (view, sampler)
}

fn create_shape_atlas_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &ShapeAtlas,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solido_shape_atlas"),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width * 4),
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("solido_shape_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });

    (view, sampler)
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    organism_buffer: &wgpu::Buffer,
    font_atlas_view: &wgpu::TextureView,
    font_sampler: &wgpu::Sampler,
    glyph_buffer: &wgpu::Buffer,
    shape_atlas_view: &wgpu::TextureView,
    shape_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("solido_organism_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: organism_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(font_atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(font_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: glyph_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(shape_atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(shape_sampler),
            },
        ],
    })
}

// ============================================================================
// Per-frame callback
// ============================================================================

pub struct OrganismCallback {
    pub uniforms: Uniforms,
    pub organisms: Vec<OrganismGpuData>,
    pub glyphs: Vec<TextGlyphGpu>,
    pub capture_requested: bool,
    pub capture_width: u32,
    pub capture_height: u32,
}

impl egui_wgpu::CallbackTrait for OrganismCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut OrganismRenderResources = callback_resources
            .get_mut()
            .expect("OrganismRenderResources not found");

        queue.write_buffer(
            &resources.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.uniforms),
        );

        // Reallocate organism buffer if needed
        let org_count = self.organisms.len();
        let glyph_count = self.glyphs.len();
        let mut needs_rebind = false;

        if org_count > resources.organism_capacity {
            let new_capacity = (org_count * 2).max(INITIAL_ORGANISM_CAPACITY);
            let new_size = (mem::size_of::<OrganismGpuData>() * new_capacity) as u64;
            resources.organism_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solido_organism_buf"),
                size: new_size.max(MIN_STORAGE_BYTES),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            resources.organism_capacity = new_capacity;
            needs_rebind = true;
        }

        // Reallocate glyph buffer if needed
        if glyph_count > resources.glyph_capacity {
            let new_capacity = (glyph_count * 2).max(INITIAL_GLYPH_CAPACITY);
            let new_size = (mem::size_of::<TextGlyphGpu>() * new_capacity) as u64;
            resources.glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solido_glyph_buf"),
                size: new_size.max(MIN_STORAGE_BYTES),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            resources.glyph_capacity = new_capacity;
            needs_rebind = true;
        }

        if needs_rebind {
            resources.bind_group = create_bind_group(
                device,
                &resources.bind_group_layout,
                &resources.uniform_buffer,
                &resources.organism_buffer,
                &resources.font_atlas_view,
                &resources.font_sampler,
                &resources.glyph_buffer,
                &resources.shape_atlas_view,
                &resources.shape_sampler,
            );
        }

        if !self.organisms.is_empty() {
            queue.write_buffer(
                &resources.organism_buffer,
                0,
                bytemuck::cast_slice(&self.organisms),
            );
        }

        if !self.glyphs.is_empty() {
            queue.write_buffer(
                &resources.glyph_buffer,
                0,
                bytemuck::cast_slice(&self.glyphs),
            );
        }

        // -- Offscreen capture render pass ------------------------------------
        if self.capture_requested && self.capture_width > 0 && self.capture_height > 0 {
            let w = self.capture_width;
            let h = self.capture_height;

            // Lazily create / resize offscreen texture + staging buffer
            if resources.capture_width != w || resources.capture_height != h {
                let tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("solido_capture_tex"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                resources.capture_texture_view =
                    Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
                resources.capture_texture = Some(tex);

                let padded_row = padded_bytes_per_row(w);
                let buf_size = (padded_row * h) as u64;
                resources.staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("solido_staging_buf"),
                    size: buf_size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));

                resources.capture_width = w;
                resources.capture_height = h;
            }

            let view = resources.capture_texture_view.as_ref().unwrap();
            let staging = resources.staging_buffer.as_ref().unwrap();

            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("solido_capture_encoder"),
                });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("solido_capture_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });
                pass.set_pipeline(&resources.capture_pipeline);
                pass.set_bind_group(0, &resources.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            let padded_row = padded_bytes_per_row(w);
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: resources.capture_texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row as u32),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );

            return vec![encoder.finish()];
        }

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let resources: &OrganismRenderResources = callback_resources
            .get()
            .expect("OrganismRenderResources not found");

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

// ============================================================================
// Public helper
// ============================================================================

pub fn create_paint_callback(
    uniforms: Uniforms,
    organisms: Vec<OrganismGpuData>,
    glyphs: Vec<TextGlyphGpu>,
    rect: egui::Rect,
    capture_requested: bool,
    capture_width: u32,
    capture_height: u32,
) -> egui::PaintCallback {
    egui_wgpu::Callback::new_paint_callback(
        rect,
        OrganismCallback {
            uniforms,
            organisms,
            glyphs,
            capture_requested,
            capture_width,
            capture_height,
        },
    )
}

// ============================================================================
// Row-alignment helper (wgpu COPY_BYTES_PER_ROW_ALIGNMENT = 256)
// ============================================================================

fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    (unpadded + align - 1) & !(align - 1)
}

// ============================================================================
// Deferred readback — call one frame after capture
// ============================================================================

pub fn read_captured_frame(
    device: &wgpu::Device,
    resources: &OrganismRenderResources,
    frame_number: u32,
    timestamp: f32,
) -> Option<CapturedFrame> {
    let staging = resources.staging_buffer.as_ref()?;
    let w = resources.capture_width;
    let h = resources.capture_height;
    if w == 0 || h == 0 {
        return None;
    }

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    rx.recv().ok()?.ok()?;

    let data = slice.get_mapped_range();
    let padded_row = padded_bytes_per_row(w) as usize;
    let unpadded_row = (w * 4) as usize;

    // Strip row padding + un-premultiply alpha
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let row_start = y * padded_row;
        let row = &data[row_start..row_start + unpadded_row];
        pixels.extend_from_slice(row);
    }
    drop(data);
    staging.unmap();

    // Un-premultiply alpha
    for chunk in pixels.chunks_exact_mut(4) {
        let a = chunk[3] as f32;
        if a > 0.0 {
            let inv_a = 255.0 / a;
            chunk[0] = ((chunk[0] as f32 * inv_a).round() as u32).min(255) as u8;
            chunk[1] = ((chunk[1] as f32 * inv_a).round() as u32).min(255) as u8;
            chunk[2] = ((chunk[2] as f32 * inv_a).round() as u32).min(255) as u8;
        }
    }

    Some(CapturedFrame {
        pixels,
        width: w,
        height: h,
        frame_number,
        timestamp_secs: timestamp,
    })
}
