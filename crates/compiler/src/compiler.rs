use crate::guestmem::GuestMem;
use crate::pe::{self, PeInfo};
use crate::translate::program::{translate_program, ModuleImage};
use serde::Serialize;
use std::fmt;

/// An invalid executable or module mapping.
#[derive(Debug)]
pub struct CompileError(pub(crate) String);

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CompileError {}

impl From<String> for CompileError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Addresses required to link the emitted module to the guest runtime.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub diagnostics: Vec<String>,
    pub imports: Vec<crate::pe::PeImport>,
    pub modules: Vec<ModuleMetadata>,
    pub instruction_maps: Vec<(u32, Vec<crate::translate::program::InsnMapEntry>)>,
    pub roots: Vec<u32>,
    pub func_addrs: Vec<u32>,
    pub table_addr: u32,
    pub command_line_addr: u32,
    pub tramp_base: u32,
    pub entry: u32,
    pub func_count: u32,
    pub func_body_starts: Vec<usize>,
    pub blocks_by_func: Vec<(u32, Vec<u32>)>,
    pub wasm_bytes: usize,
}

/// Load addresses and import slots for an additional mapped PE image.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleMetadata {
    pub name: Option<String>,
    pub entry: Option<u32>,
    pub is_dll: bool,
    pub exports: Vec<crate::pe::PeExport>,
    pub base: u32,
    pub preferred_base: u32,
    pub size_of_image: u32,
    pub imports: Vec<crate::pe::PeImport>,
}

impl ModuleMetadata {
    fn mapped(base: u32, info: &PeInfo) -> Self {
        let imports = info
            .imports
            .iter()
            .cloned()
            .map(|mut import| {
                import.iat_va = base + (import.iat_va - info.image_base);
                import
            })
            .collect();
        let exports = info
            .exports
            .iter()
            .cloned()
            .map(|mut export| {
                if let pe::ExportTarget::Address { address } = &mut export.target {
                    *address = base + (*address - info.image_base);
                }
                export
            })
            .collect();
        Self {
            name: info.name.clone(),
            entry: (info.entry_va != info.image_base)
                .then(|| base + (info.entry_va - info.image_base)),
            is_dll: info.is_dll,
            exports,
            base,
            preferred_base: info.image_base,
            size_of_image: info.size_of_image,
            imports,
        }
    }
}

/// Generated module and its linking metadata.
pub struct Artifact {
    pub wasm: Vec<u8>,
    pub metadata: Metadata,
}

impl Artifact {
    pub fn metadata_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.metadata)
    }

    /// Runtime linking metadata without instruction maps or compiler diagnostics.
    pub fn runtime_metadata_json(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RuntimeMetadata<'a> {
            func_addrs: &'a [u32],
            func_count: u32,
            table_addr: u32,
            tramp_base: u32,
            entry: u32,
            command_line_addr: u32,
        }

        let metadata = &self.metadata;
        serde_json::to_string(&RuntimeMetadata {
            func_addrs: &metadata.func_addrs,
            func_count: metadata.func_count,
            table_addr: metadata.table_addr,
            tramp_base: metadata.tramp_base,
            entry: metadata.entry,
            command_line_addr: metadata.command_line_addr,
        })
    }
}

/// Owns a sparse guest image; the compiler's memory is separate from guest execution.
pub struct Compiler {
    memory: GuestMem,
    executable: PeInfo,
    modules: Vec<(u32, PeInfo)>,
    relocation_spans: Vec<(u32, u32)>,
}

impl Compiler {
    pub fn new(executable: &[u8]) -> Result<Self, CompileError> {
        let info = pe::parse(executable)?;
        let mut memory = GuestMem::new();
        map_image(&mut memory, executable, &info, info.image_base)?;
        Ok(Self {
            memory,
            executable: info,
            modules: Vec::new(),
            relocation_spans: Vec::new(),
        })
    }

    /// Map a DLL at its runtime load address before discovering its functions.
    pub fn add_module(&mut self, bytes: &[u8], base: u32) -> Result<(), CompileError> {
        let info = pe::parse(bytes)?;
        let relocations = pe::relocations(bytes, &info, base)?;
        let start = u64::from(base);
        let end = start + u64::from(info.size_of_image);
        let overlaps = |existing_base: u32, existing_size: u32| {
            start < u64::from(existing_base) + u64::from(existing_size)
                && u64::from(existing_base) < end
        };
        if overlaps(self.executable.image_base, self.executable.size_of_image)
            || self
                .modules
                .iter()
                .any(|(base, module)| overlaps(*base, module.size_of_image))
        {
            return Err(CompileError(format!(
                "module at {base:#x} overlaps an existing image"
            )));
        }
        map_image(&mut self.memory, bytes, &info, base)?;
        let delta = base.wrapping_sub(info.image_base);
        for address in relocations {
            let value = self.memory.u32_le(address as usize).wrapping_add(delta);
            self.memory.write_u32(address as usize, value);
            let rva = address - base;
            let initialized = rva + 4 <= info.size_of_headers
                || info.sections.iter().any(|section| {
                    rva >= section.rva && rva + 4 <= section.rva + section.size_of_raw_data
                });
            if !initialized {
                self.relocation_spans.push((address, 4));
            }
        }
        self.modules.push((base, info));
        Ok(())
    }

    /// Snapshot mapped PE bytes after relocation, separate from compiler-owned memory.
    /// Zero each image range before copying the returned initialized spans.
    pub fn load_plan(&self) -> crate::LoadPlan {
        let images = std::iter::once((self.executable.image_base, &self.executable))
            .chain(self.modules.iter().map(|(base, info)| (*base, info)));
        let mut plan = crate::LoadPlan::default();
        for (base, info) in images {
            plan.images.push(ModuleMetadata::mapped(base, info));
            plan.append(&self.memory, base, info.size_of_headers);
            for section in &info.sections {
                plan.append(&self.memory, base + section.rva, section.size_of_raw_data);
            }
            // Relocations in zero-filled tails must also be transferred.
            for &(address, length) in &self.relocation_spans {
                if address >= base
                    && u64::from(address) < u64::from(base) + u64::from(info.size_of_image)
                {
                    plan.append(&self.memory, address, length);
                }
            }
        }
        plan
    }

    /// Compile the entry point and explicitly supplied callback roots.
    pub fn compile(&self, roots: &[u32]) -> Result<Artifact, CompileError> {
        let modules: Vec<_> = self
            .modules
            .iter()
            .map(|(base, info)| ModuleImage {
                base: *base,
                size_of_image: info.size_of_image,
                info,
            })
            .collect();
        let program = translate_program(
            &self.memory,
            0,
            self.executable.entry_va,
            &self.executable,
            roots,
            &modules,
        )
        .map_err(|error| CompileError(error.to_string()))?;
        if program
            .func_addrs
            .binary_search(&self.executable.entry_va)
            .is_err()
        {
            let entry = self.executable.entry_va;
            let prefix = format!("skipped function {entry:#x}:");
            let reason = program
                .diagnostics
                .iter()
                .find(|message| message.starts_with(&prefix))
                .map(String::as_str)
                .unwrap_or("entry function was not discovered");
            return Err(CompileError(format!(
                "cannot compile entry point {entry:#x}: {reason}"
            )));
        }
        // Guest modules use WebAssembly 1.0: numeric instructions, one memory,
        // a funcref call table, and imported mutable register globals. Keep newer
        // proposals disabled until an emitted instruction explicitly requires one.
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
            .validate_all(&program.wasm)
            .map_err(|error| {
                let index = program
                    .func_body_starts
                    .partition_point(|&start| start <= error.offset());
                let context = index
                    .checked_sub(1)
                    .and_then(|index| {
                        let (address, instructions) = program.insn_maps.get(index)?;
                        let offset = error.offset() - program.func_body_starts[index];
                        let instruction = instructions
                            .iter()
                            .rev()
                            .find(|insn| insn.wasm_off as usize <= offset);
                        Some(match instruction {
                            Some(insn) => format!(
                                " in function {address:#x} near instruction {:#x}",
                                insn.guest_va
                            ),
                            None => format!(" in function {address:#x}"),
                        })
                    })
                    .unwrap_or_default();
                CompileError(format!("invalid emitted WebAssembly{context}: {error}"))
            })?;
        let metadata = Metadata {
            diagnostics: program.diagnostics,
            imports: self.executable.imports.clone(),
            modules: self
                .modules
                .iter()
                .map(|(base, info)| ModuleMetadata::mapped(*base, info))
                .collect(),
            instruction_maps: program.insn_maps,
            roots: roots.to_vec(),
            func_addrs: program.func_addrs,
            table_addr: program.table_addr,
            command_line_addr: crate::translate::abi::COMMAND_LINE_ADDR,
            tramp_base: program.tramp_base,
            entry: program.entry,
            func_count: program.table_func_count,
            func_body_starts: program.func_body_starts,
            blocks_by_func: program.blocks_by_func,
            wasm_bytes: program.wasm.len(),
        };
        Ok(Artifact {
            wasm: program.wasm,
            metadata,
        })
    }
}

fn map_image(
    memory: &mut GuestMem,
    bytes: &[u8],
    info: &PeInfo,
    base: u32,
) -> Result<(), CompileError> {
    let image_end = u64::from(base) + u64::from(info.size_of_image);
    if image_end > crate::guestmem::GUEST_SIZE as u64 {
        return Err(CompileError("image exceeds guest address space".into()));
    }
    // These ranges belong to the browser host, not to PE images. Reserve a
    // megabyte for the function-address table, whose actual size is known only
    // after discovery. The runtime also checks its exact mapped extent.
    for (start, end, name) in [
        (0, 0x1_0000, "null page"),
        (
            crate::translate::abi::API_BASE,
            crate::translate::abi::API_END,
            "API thunks",
        ),
        (
            crate::translate::abi::TAB_ADDR,
            crate::translate::abi::TAB_ADDR + 0x10_0000,
            "function table",
        ),
        (
            crate::translate::abi::COMMAND_LINE_ADDR,
            crate::translate::abi::COMMAND_LINE_ADDR + 4096,
            "command line",
        ),
        (0xe0_0000, 0xf0_0000, "main-thread stack"),
        (0x1f00_0000, 0x1f01_0000, "main-thread TEB"),
    ] {
        if u64::from(base) < u64::from(end) && u64::from(start) < image_end {
            return Err(CompileError(format!(
                "image at {base:#x} overlaps host {name}"
            )));
        }
    }
    if info.size_of_headers > info.size_of_image {
        return Err(CompileError("headers exceed image size".into()));
    }
    let headers = bytes
        .get(..info.size_of_headers as usize)
        .ok_or_else(|| CompileError("truncated PE headers".into()))?;
    // Validate all source ranges before mutating the image.
    let mut sections = Vec::new();
    for section in &info.sections {
        if section.size_of_raw_data == 0 {
            continue;
        }
        let start = section.file_offset as usize;
        let end = start
            .checked_add(section.size_of_raw_data as usize)
            .ok_or_else(|| CompileError("section range overflow".into()))?;
        let data = bytes
            .get(start..end)
            .ok_or_else(|| CompileError("truncated PE section".into()))?;
        let address = u64::from(base) + u64::from(section.rva);
        if address + data.len() as u64 > image_end {
            return Err(CompileError("section exceeds image size".into()));
        }
        sections.push((address as usize, data));
    }
    memory.write(base as usize, headers);
    for (address, data) in sections {
        memory.write(address, data);
    }
    Ok(())
}
