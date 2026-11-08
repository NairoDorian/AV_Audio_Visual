//! The native renderer (blueprint A9): scene + frame -> triangles -> a wgpu target.
//!
//! [`mesh`] turns a scene, the output bins and the raw-audio window into premultiplied triangles
//! in clip space (pure, allocation-free once warm, the same geometry as the Studio preview).
//! [`gpu`] owns the device, the one pipeline, window targets and offscreen capture. Windows come
//! from `av-platform` as raw handles; this crate knows nothing about hosts.

pub mod color;
pub mod gpu;
pub mod mesh;

pub use gpu::{Gpu, RenderError, SurfaceTarget, TargetSpec, block_on, instance};
pub use mesh::{Composer, Mesh, Vertex, View};
