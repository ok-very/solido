// Blob SDF render pipeline for egui-wgpu.
//
// Renders organisms as multi-lobe metaballs using smooth-minimum (smin)
// blending. Each organism's lobes are circle SDFs blended with smin_k,
// producing amoeba-like silhouettes. Thermal palette coloring is driven
// by emotion arousal, with beat-synced pulsing and glow halos.
//
// This replaces the L-shaped organism renderer with proper blob geometry.
// The old organism_renderer.rs is kept for backward compatibility.

use bytemuck::{Pod, Zeroable};
use std::mem;
use wgpu::util::DeviceExt;

use super::font_atlas::FontAtlas;
use super::shape_atlas::ShapeAtlas;
use crate::recorder::CapturedFrame;

// ============================================================================
// GPU-side data structures (must match blob.wgsl exactly)
// ============================================================================

/// Uniform buffer layout -- 32 bytes, two 16-byte rows.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct BlobUniforms {
    pub viewport: [f32; 2],
    pub time: f32,
    pub organism_count: f32,
    pub dpr: f32,
    pub beat_phase: f32,
    pub gravity_strength: f32,
    pub cross_smin_k: f32,
}

/// Per-organism data -- 48 bytes, three 16-byte rows.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct BlobOrgData {
    pub pos: [f32; 2],
    pub smin_k: f32,
    pub edge_softness: f32,
    pub thermal_temp: f32,
    pub hue_shift: f32,
    pub pulse_phase: f32,
    pub pulse_amp: f32,
    pub glow: f32,
    pub lobe_start: u32,
    pub lobe_count: u32,
    pub glob_group: u32,
}

/// Per-lobe data -- 16 bytes, one 16-byte row.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LobeGpu {
    pub offset: [f32; 2],
    pub radius: f32,
    pub _pad: f32,
}

/// Per-glyph instance data -- 32 bytes (reused from organism_renderer).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TextGlyphGpu {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv_rect: [f32; 4],
}

// Compile-time layout assertions
const _: () = assert!(mem::size_of::<BlobUniforms>() == 32);
const _: () = assert!(mem::size_of::<BlobOrgData>() == 48);
const _: () = assert!(mem::size_of::<LobeGpu>() == 16);
const _: () = assert!(mem::size_of::<TextGlyphGpu>() == 32);

// ============================================================================
// Persistent GPU resources (stored in CallbackResources)
// ============================================================================

pub struct BlobRenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    organism_buffer: wgpu::Buffer,
    lobe_buffer: wgpu::Buffer,
    glyph_buffer: wgpu::Buffer,
    font_atlas_view: wgpu::TextureView,
    font_sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    organism_capacity: usize,
    lobe_capacity: usize,
    glyph_capacity: usize,
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
const INITIAL_LOBE_CAPACITY: usize = 192; // 16 organisms * 12 lobes
const INITIAL_GLYPH_CAPACITY: usize = 128;

pub fn init_resources(
    render_state: &egui_wgpu::RenderState,
    atlas: &FontAtlas,
    shape_atlas: &ShapeAtlas,
) {
    let device = &render_state.device;
    let queue = &render_state.queue;

    let shader = device.create_shader_module(wgpu::include_wgsl!("blob.wgsl"));

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("solido_blob_bgl"),
        entries: &[
            // binding 0: BlobUniforms
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        mem::size_of::<BlobUniforms>() as u64,
                    ),
                },
                count: None,
            },
            // binding 1: BlobOrgData[]
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        mem::size_of::<BlobOrgData>() as u64,
                    ),
                },
                count: None,
            },
            // binding 2: LobeGpu[]
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        mem::size_of::<LobeGpu>() as u64,
                    ),
                },
                count: None,
            },
            // binding 3: SDF font atlas texture
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // binding 4: sampler for atlas
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // binding 5: TextGlyph[]
            wgpu::BindGroupLayoutEntry {
                binding: 5,
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
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("solido_blob_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solido_blob_pipeline"),
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
        label: Some("solido_blob_capture_pipeline"),
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
        label: Some("solido_blob_uniform_buf"),
        contents: bytemuck::bytes_of(&BlobUniforms::zeroed()),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let organism_buf_size =
        (mem::size_of::<BlobOrgData>() * INITIAL_ORGANISM_CAPACITY) as u64;
    let organism_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("solido_blob_organism_buf"),
        size: organism_buf_size.max(MIN_STORAGE_BYTES),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let lobe_buf_size = (mem::size_of::<LobeGpu>() * INITIAL_LOBE_CAPACITY) as u64;
    let lobe_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("solido_blob_lobe_buf"),
        size: lobe_buf_size.max(MIN_STORAGE_BYTES),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let glyph_buf_size =
        (mem::size_of::<TextGlyphGpu>() * INITIAL_GLYPH_CAPACITY) as u64;
    let glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("solido_blob_glyph_buf"),
        size: glyph_buf_size.max(MIN_STORAGE_BYTES),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // -- Font atlas texture --------------------------------------------------
    let (font_atlas_view, font_sampler) = create_font_atlas_texture(device, queue, atlas);

    // -- Bind group ----------------------------------------------------------
    let bind_group = create_bind_group(
        device,
        &bind_group_layout,
        &uniform_buffer,
        &organism_buffer,
        &lobe_buffer,
        &font_atlas_view,
        &font_sampler,
        &glyph_buffer,
    );

    // Shape atlas not used in blob pipeline but we keep the parameter for API compat
    let _ = shape_atlas;

    let resources = BlobRenderResources {
        pipeline,
        bind_group_layout,
        uniform_buffer,
        organism_buffer,
        lobe_buffer,
        glyph_buffer,
        font_atlas_view,
        font_sampler,
        bind_group,
        organism_capacity: INITIAL_ORGANISM_CAPACITY,
        lobe_capacity: INITIAL_LOBE_CAPACITY,
        glyph_capacity: INITIAL_GLYPH_CAPACITY,
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
        label: Some("solido_blob_font_atlas"),
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
        label: Some("solido_blob_font_sampler"),
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
    lobe_buffer: &wgpu::Buffer,
    font_atlas_view: &wgpu::TextureView,
    font_sampler: &wgpu::Sampler,
    glyph_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("solido_blob_bg"),
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
                resource: lobe_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(font_atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(font_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: glyph_buffer.as_entire_binding(),
            },
        ],
    })
}

// ============================================================================
// Per-frame callback
// ============================================================================

pub struct BlobCallback {
    pub uniforms: BlobUniforms,
    pub organisms: Vec<BlobOrgData>,
    pub lobes: Vec<LobeGpu>,
    pub glyphs: Vec<TextGlyphGpu>,
    pub capture_requested: bool,
    pub capture_width: u32,
    pub capture_height: u32,
}

impl egui_wgpu::CallbackTrait for BlobCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut BlobRenderResources = callback_resources
            .get_mut()
            .expect("BlobRenderResources not found");

        queue.write_buffer(
            &resources.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.uniforms),
        );

        let org_count = self.organisms.len();
        let lobe_count = self.lobes.len();
        let glyph_count = self.glyphs.len();
        let mut needs_rebind = false;

        // Reallocate organism buffer if needed
        if org_count > resources.organism_capacity {
            let new_capacity = (org_count * 2).max(INITIAL_ORGANISM_CAPACITY);
            let new_size = (mem::size_of::<BlobOrgData>() * new_capacity) as u64;
            resources.organism_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solido_blob_organism_buf"),
                size: new_size.max(MIN_STORAGE_BYTES),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            resources.organism_capacity = new_capacity;
            needs_rebind = true;
        }

        // Reallocate lobe buffer if needed
        if lobe_count > resources.lobe_capacity {
            let new_capacity = (lobe_count * 2).max(INITIAL_LOBE_CAPACITY);
            let new_size = (mem::size_of::<LobeGpu>() * new_capacity) as u64;
            resources.lobe_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solido_blob_lobe_buf"),
                size: new_size.max(MIN_STORAGE_BYTES),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            resources.lobe_capacity = new_capacity;
            needs_rebind = true;
        }

        // Reallocate glyph buffer if needed
        if glyph_count > resources.glyph_capacity {
            let new_capacity = (glyph_count * 2).max(INITIAL_GLYPH_CAPACITY);
            let new_size = (mem::size_of::<TextGlyphGpu>() * new_capacity) as u64;
            resources.glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solido_blob_glyph_buf"),
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
                &resources.lobe_buffer,
                &resources.font_atlas_view,
                &resources.font_sampler,
                &resources.glyph_buffer,
            );
        }

        if !self.organisms.is_empty() {
            queue.write_buffer(
                &resources.organism_buffer,
                0,
                bytemuck::cast_slice(&self.organisms),
            );
        }

        if !self.lobes.is_empty() {
            queue.write_buffer(
                &resources.lobe_buffer,
                0,
                bytemuck::cast_slice(&self.lobes),
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

            if resources.capture_width != w || resources.capture_height != h {
                let tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("solido_blob_capture_tex"),
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
                resources.staging_buffer =
                    Some(device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("solido_blob_staging_buf"),
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
                    label: Some("solido_blob_capture_encoder"),
                });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("solido_blob_capture_pass"),
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
        let resources: &BlobRenderResources = callback_resources
            .get()
            .expect("BlobRenderResources not found");

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

// ============================================================================
// Public helpers
// ============================================================================

pub fn create_paint_callback(
    uniforms: BlobUniforms,
    organisms: Vec<BlobOrgData>,
    lobes: Vec<LobeGpu>,
    glyphs: Vec<TextGlyphGpu>,
    rect: egui::Rect,
    capture_requested: bool,
    capture_width: u32,
    capture_height: u32,
) -> egui::PaintCallback {
    egui_wgpu::Callback::new_paint_callback(
        rect,
        BlobCallback {
            uniforms,
            organisms,
            lobes,
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
// Deferred readback
// ============================================================================

pub fn read_captured_frame(
    device: &wgpu::Device,
    resources: &BlobRenderResources,
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
