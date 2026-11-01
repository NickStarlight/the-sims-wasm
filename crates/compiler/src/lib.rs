//! Compile PE32 x86 images to WebAssembly, without filesystem or browser dependencies.

mod guestmem;
mod pe;
mod translate;
mod wasm;
mod x86;

#[cfg(feature = "browser")]
mod browser;
mod compiler;

pub use compiler::{Artifact, CompileError, Compiler, Metadata, ModuleMetadata};

mod profile;
pub use profile::complete_profile_json;

mod load;
pub use load::{LoadPlan, LoadSpan};
pub use pe::{ExportTarget, PeExport};
