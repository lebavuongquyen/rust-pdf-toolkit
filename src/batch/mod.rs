#[cfg(not(target_arch = "wasm32"))]
pub mod folder;

#[cfg(not(target_arch = "wasm32"))]
pub use folder::*;
