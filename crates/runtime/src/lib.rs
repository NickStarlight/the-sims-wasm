//! Rust owns process metadata in its own memory; guest bytes contain no allocator links.
mod allocation;
#[cfg(all(feature = "browser", target_arch = "wasm32"))]
mod browser;
mod callbacks;
mod cpu;
mod display;
mod error;
mod files;
mod gdi;
mod graphics;
mod memory;
mod modules;
mod process;
mod regions;
mod registry;
mod user;

pub use error::{Error, Result};
pub use memory::{GuestMemory, SparseMemory, U32Update};
pub use process::Process;
pub use regions::{Region, RegionKind};

pub mod kernel;

pub use callbacks::CallOutcome;

pub use graphics::{Frame, PackedFrame};

pub mod audio;

mod media;
pub use media::MediaCallback;
