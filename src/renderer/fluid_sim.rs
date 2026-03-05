// Fluid simulation constants shared between Rust and WGSL.
//
// The actual simulation runs entirely on GPU via fluid_sim.wgsl.
// This module exists for the mod.rs registration; all GPU resource
// management lives in biofield_renderer.rs alongside the rest of
// the render pipeline.

use bytemuck::{Pod, Zeroable};

/// Uniform buffer for fluid simulation passes — 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FluidUniforms {
    pub viewport:   [f32; 2],
    pub time:       f32,
    pub dt:         f32,
    pub texel_size: [f32; 2],
    pub cell_count: f32,
    pub _pad:       f32,
}

const _: () = assert!(std::mem::size_of::<FluidUniforms>() == 32);

/// Number of pressure SOR iterations per frame.
pub const PRESSURE_ITERATIONS: u32 = 2;
