// BioField 3-layer render pipeline for egui-wgpu.
//
// Layer 1 (Background): Generated inline in composite shader.
// Layer 2 (BioField):   Organisms rendered to intermediate RGBA16Float texture.
//                        SDF metaballs + spectral paint mixing + paraboloid specular.
// Layer 3 (Composite):  Samples biofield texture, composites over checkerboard background.
//
// prepare() runs the BioField pass → intermediate texture (+ optional capture pass).
// paint()  runs the Composite pass → screen (egui render pass).

use bytemuck::{Pod, Zeroable};
use std::mem;
use wgpu::util::DeviceExt;

use crate::recorder::CapturedFrame;

// ============================================================================
// GPU-side data structures (must match biofield.wgsl exactly)
// ============================================================================

/// Uniform buffer for biofield pass — 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct BioFieldUniforms {
    pub viewport:   [f32; 2],
    pub time:       f32,
    pub cell_count: f32,
}

/// Uniform buffer for composite pass — 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CompositeUniforms {
    pub viewport: [f32; 2],
    pub time:     f32,
    pub _pad:     f32,
}

/// Per-organism cell data — 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CellData {
    pub pos:          [f32; 2],
    pub radius:       f32,
    pub audio_energy: f32,
    pub cell_id:      u32,
    pub hue:          f32,
    pub vel:          [f32; 2],
}

const _: () = assert!(mem::size_of::<BioFieldUniforms>() == 16);
const _: () = assert!(mem::size_of::<CompositeUniforms>() == 16);
const _: () = assert!(mem::size_of::<CellData>() == 32);

// ============================================================================
// Persistent GPU resources
// ============================================================================

pub struct BioFieldRenderResources {
    // -- BioField pass (organisms → intermediate texture) --
    biofield_pipeline:         wgpu::RenderPipeline,
    biofield_bind_group_layout: wgpu::BindGroupLayout,
    biofield_bind_group:       wgpu::BindGroup,
    uniform_buffer:            wgpu::Buffer,
    cell_buffer:               wgpu::Buffer,
    cell_capacity:             usize,

    // -- Intermediate texture (RGBA16Float, recreated on resize) --
    biofield_texture:          wgpu::Texture,
    biofield_texture_view:     wgpu::TextureView,
    biofield_width:            u32,
    biofield_height:           u32,

    // -- Composite pass (background + biofield → screen) --
    composite_pipeline:         wgpu::RenderPipeline,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_bind_group:       wgpu::BindGroup,
    composite_uniform_buffer:   wgpu::Buffer,
    sampler:                    wgpu::Sampler,

    // -- Capture pipeline (Rgba8Unorm, transparent bg) --
    capture_pipeline:       wgpu::RenderPipeline,
    capture_texture:        Option<wgpu::Texture>,
    capture_texture_view:   Option<wgpu::TextureView>,
    staging_buffer:         Option<wgpu::Buffer>,
    capture_width:          u32,
    capture_height:         u32,
}

// ============================================================================
// Initialization
// ============================================================================

const MIN_STORAGE_BYTES: u64 = 32;
const INITIAL_CELL_CAPACITY: usize = 16;
const INITIAL_TEX_WIDTH: u32 = 4;
const INITIAL_TEX_HEIGHT: u32 = 4;

pub fn init_resources(render_state: &egui_wgpu::RenderState) {
    let device = &render_state.device;

    let biofield_shader = device.create_shader_module(wgpu::include_wgsl!("biofield.wgsl"));
    let composite_shader = device.create_shader_module(wgpu::include_wgsl!("composite.wgsl"));

    // ── BioField bind group layout ──────────────────────────────────────────
    let biofield_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("solido_biofield_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(mem::size_of::<BioFieldUniforms>() as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(mem::size_of::<CellData>() as u64),
                },
                count: None,
            },
        ],
    });

    let biofield_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("solido_biofield_pipeline_layout"),
        bind_group_layouts: &[&biofield_bgl],
        push_constant_ranges: &[],
    });

    // BioField pipeline — renders to RGBA16Float intermediate texture
    let biofield_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solido_biofield_pipeline"),
        layout: Some(&biofield_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &biofield_shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: fullscreen_primitive(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &biofield_shader,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    });

    // Capture pipeline — renders to Rgba8Unorm for video export
    let capture_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solido_biofield_capture_pipeline"),
        layout: Some(&biofield_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &biofield_shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: fullscreen_primitive(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &biofield_shader,
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

    // ── Composite bind group layout ─────────────────────────────────────────
    let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("solido_composite_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(mem::size_of::<CompositeUniforms>() as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let composite_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("solido_composite_pipeline_layout"),
        bind_group_layouts: &[&composite_bgl],
        push_constant_ranges: &[],
    });

    // Composite pipeline — renders to swapchain (egui render pass)
    let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solido_composite_pipeline"),
        layout: Some(&composite_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &composite_shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: fullscreen_primitive(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &composite_shader,
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

    // ── Buffers ─────────────────────────────────────────────────────────────
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("solido_biofield_uniform_buf"),
        contents: bytemuck::bytes_of(&BioFieldUniforms::zeroed()),
        usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let cell_buf_size = (mem::size_of::<CellData>() * INITIAL_CELL_CAPACITY) as u64;
    let cell_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label:              Some("solido_biofield_cell_buf"),
        size:               cell_buf_size.max(MIN_STORAGE_BYTES),
        usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let biofield_bind_group = create_biofield_bind_group(device, &biofield_bgl, &uniform_buffer, &cell_buffer);

    // ── Intermediate texture ────────────────────────────────────────────────
    let (biofield_texture, biofield_texture_view) = create_biofield_texture(device, INITIAL_TEX_WIDTH, INITIAL_TEX_HEIGHT);

    // ── Sampler ─────────────────────────────────────────────────────────────
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("solido_biofield_sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // ── Composite buffers + bind group ──────────────────────────────────────
    let composite_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("solido_composite_uniform_buf"),
        contents: bytemuck::bytes_of(&CompositeUniforms::zeroed()),
        usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let composite_bind_group = create_composite_bind_group(
        device, &composite_bgl, &composite_uniform_buffer, &biofield_texture_view, &sampler,
    );

    // ── Store resources ─────────────────────────────────────────────────────
    let resources = BioFieldRenderResources {
        biofield_pipeline,
        biofield_bind_group_layout: biofield_bgl,
        biofield_bind_group,
        uniform_buffer,
        cell_buffer,
        cell_capacity: INITIAL_CELL_CAPACITY,

        biofield_texture,
        biofield_texture_view,
        biofield_width: INITIAL_TEX_WIDTH,
        biofield_height: INITIAL_TEX_HEIGHT,

        composite_pipeline,
        composite_bind_group_layout: composite_bgl,
        composite_bind_group,
        composite_uniform_buffer,
        sampler,

        capture_pipeline,
        capture_texture: None,
        capture_texture_view: None,
        staging_buffer: None,
        capture_width: 0,
        capture_height: 0,
    };

    render_state.renderer.write().callback_resources.insert(resources);
}

fn fullscreen_primitive() -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
    }
}

fn create_biofield_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    cell_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("solido_biofield_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: cell_buffer.as_entire_binding() },
        ],
    })
}

fn create_composite_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("solido_composite_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(texture_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn create_biofield_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solido_biofield_intermediate_tex"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

// ============================================================================
// Per-frame callback
// ============================================================================

pub struct BioFieldCallback {
    pub uniforms:          BioFieldUniforms,
    pub cells:             Vec<CellData>,
    pub capture_requested: bool,
    pub capture_width:     u32,
    pub capture_height:    u32,
}

impl egui_wgpu::CallbackTrait for BioFieldCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut BioFieldRenderResources = callback_resources
            .get_mut()
            .expect("BioFieldRenderResources not found");

        // Upload biofield uniforms
        queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));

        // Upload composite uniforms
        let composite_uniforms = CompositeUniforms {
            viewport: self.uniforms.viewport,
            time:     self.uniforms.time,
            _pad:     0.0,
        };
        queue.write_buffer(&resources.composite_uniform_buffer, 0, bytemuck::bytes_of(&composite_uniforms));

        // Reallocate cell buffer if needed
        let cell_count = self.cells.len();
        let mut needs_rebind = false;

        if cell_count > resources.cell_capacity {
            let new_capacity = (cell_count * 2).max(INITIAL_CELL_CAPACITY);
            let new_size = (mem::size_of::<CellData>() * new_capacity) as u64;
            resources.cell_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label:              Some("solido_biofield_cell_buf"),
                size:               new_size.max(MIN_STORAGE_BYTES),
                usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            resources.cell_capacity = new_capacity;
            needs_rebind = true;
        }

        if needs_rebind {
            resources.biofield_bind_group = create_biofield_bind_group(
                device,
                &resources.biofield_bind_group_layout,
                &resources.uniform_buffer,
                &resources.cell_buffer,
            );
        }

        if !self.cells.is_empty() {
            queue.write_buffer(&resources.cell_buffer, 0, bytemuck::cast_slice(&self.cells));
        }

        // Resize intermediate texture if viewport changed
        let vp_w = self.uniforms.viewport[0] as u32;
        let vp_h = self.uniforms.viewport[1] as u32;
        let tex_w = vp_w.max(1);
        let tex_h = vp_h.max(1);
        if tex_w != resources.biofield_width || tex_h != resources.biofield_height {
            let (tex, view) = create_biofield_texture(device, tex_w, tex_h);
            resources.biofield_texture = tex;
            resources.biofield_texture_view = view;
            resources.biofield_width = tex_w;
            resources.biofield_height = tex_h;

            // Rebuild composite bind group with new texture view
            resources.composite_bind_group = create_composite_bind_group(
                device,
                &resources.composite_bind_group_layout,
                &resources.composite_uniform_buffer,
                &resources.biofield_texture_view,
                &resources.sampler,
            );
        }

        let mut cmd_buffers = Vec::new();

        // ── BioField render pass → intermediate texture ─────────────────────
        {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("solido_biofield_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("solido_biofield_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &resources.biofield_texture_view,
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
                pass.set_pipeline(&resources.biofield_pipeline);
                pass.set_bind_group(0, &resources.biofield_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            cmd_buffers.push(encoder.finish());
        }

        // ── Capture render pass (if requested) ─────────────────────────────
        if self.capture_requested && self.capture_width > 0 && self.capture_height > 0 {
            let w = self.capture_width;
            let h = self.capture_height;

            if resources.capture_width != w || resources.capture_height != h {
                let tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("solido_biofield_capture_tex"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                resources.capture_texture_view = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
                resources.capture_texture = Some(tex);

                let padded_row = padded_bytes_per_row(w);
                resources.staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("solido_biofield_staging_buf"),
                    size: (padded_row * h) as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));

                resources.capture_width = w;
                resources.capture_height = h;
            }

            let view = resources.capture_texture_view.as_ref().unwrap();
            let staging = resources.staging_buffer.as_ref().unwrap();

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("solido_biofield_capture_encoder"),
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("solido_biofield_capture_pass"),
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
                pass.set_bind_group(0, &resources.biofield_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            let padded_row = padded_bytes_per_row(w);
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture:   resources.capture_texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin:    wgpu::Origin3d::ZERO,
                    aspect:    wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset:         0,
                        bytes_per_row:  Some(padded_row),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );

            cmd_buffers.push(encoder.finish());
        }

        cmd_buffers
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let resources: &BioFieldRenderResources = callback_resources
            .get()
            .expect("BioFieldRenderResources not found");

        // Composite pass: sample biofield texture, composite over background
        render_pass.set_pipeline(&resources.composite_pipeline);
        render_pass.set_bind_group(0, &resources.composite_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

// ============================================================================
// Public helpers
// ============================================================================

pub fn create_paint_callback(
    uniforms: BioFieldUniforms,
    cells: Vec<CellData>,
    rect: egui::Rect,
    capture_requested: bool,
    capture_width: u32,
    capture_height: u32,
) -> egui::PaintCallback {
    egui_wgpu::Callback::new_paint_callback(
        rect,
        BioFieldCallback {
            uniforms,
            cells,
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
    let align    = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    (unpadded + align - 1) & !(align - 1)
}

// ============================================================================
// Deferred readback
// ============================================================================

pub fn read_captured_frame(
    device: &wgpu::Device,
    resources: &BioFieldRenderResources,
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

    let data       = slice.get_mapped_range();
    let padded_row = padded_bytes_per_row(w) as usize;
    let unpadded_row = (w * 4) as usize;

    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let row_start = y * padded_row;
        pixels.extend_from_slice(&data[row_start..row_start + unpadded_row]);
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
