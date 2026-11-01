//! Encodes the guest module while preserving its import order and debugger offsets.

use std::{borrow::Cow, collections::BTreeMap};

use wasm_encoder::{
    ConstExpr, ElementSection, Elements, Encode, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemorySection, MemoryType, Module,
    NameMap, NameSection, RefType, Section, TableType, TypeSection,
};
use wasmparser::{Parser, Payload};

use super::{leb::uleb, opcodes::ValType};

pub struct FuncDef {
    pub type_idx: u32,
    pub locals: Vec<ValType>,
    pub body: Vec<u8>,
}

/// Encode directly into the module, avoiding a second complete code buffer.
struct StreamingCode<'a>(&'a [FuncDef], usize);

impl Encode for StreamingCode<'_> {
    fn encode(&self, sink: &mut Vec<u8>) {
        let headers: Vec<_> = self
            .0
            .iter()
            .map(|definition| {
                Function::new_with_locals_types(definition.locals.iter().copied().map(value_type))
                    .into_raw_body()
            })
            .collect();
        let length = uleb(self.0.len() as u32).len()
            + self
                .0
                .iter()
                .zip(&headers)
                .map(|(definition, header)| {
                    let body = header.len() + definition.body.len() + 1;
                    uleb(body as u32).len() + body
                })
                .sum::<usize>();
        sink.reserve_exact(uleb(length as u32).len() + length + self.1);
        length.encode(sink);
        self.0.len().encode(sink);
        for (definition, header) in self.0.iter().zip(headers) {
            (header.len() + definition.body.len() + 1).encode(sink);
            sink.extend_from_slice(&header);
            sink.extend_from_slice(&definition.body);
            sink.push(0x0b);
        }
    }
}

impl Section for StreamingCode<'_> {
    fn id(&self) -> u8 {
        10
    }
}

struct ImportedFn {
    module: &'static str,
    name: &'static str,
    type_idx: u32,
}

struct Export {
    name: &'static str,
    idx: u32,
    kind: ExportKind,
}

struct Segment {
    offset: u32,
    func_idxs: Vec<u32>,
}

pub struct WasmBuilder {
    names: BTreeMap<u32, String>,
    types: Vec<(Vec<ValType>, Vec<ValType>)>, // (params, results)
    imports: Vec<ImportedFn>,
    funcs: Vec<FuncDef>,
    exports: Vec<Export>,
    globals: Vec<(&'static str, ValType)>, // imported mutable i32 globals by name
    private_globals: Vec<i32>,
    table_min: Option<u32>,
    segments: Vec<Segment>,
    memory: Option<(u32, Option<u32>)>,
    memory_imported: bool,
    pub func_body_starts: Vec<usize>,
}

impl WasmBuilder {
    pub fn new() -> Self {
        WasmBuilder {
            names: BTreeMap::new(),
            types: Vec::new(),
            imports: Vec::new(),
            funcs: Vec::new(),
            exports: Vec::new(),
            globals: Vec::new(),
            private_globals: Vec::new(),
            table_min: None,
            segments: Vec::new(),
            memory: None,
            memory_imported: false,
            func_body_starts: Vec::new(),
        }
    }

    pub fn add_type(&mut self, params: &[ValType], results: &[ValType]) -> u32 {
        self.types.push((params.to_vec(), results.to_vec()));
        (self.types.len() - 1) as u32
    }

    /// Imported function; its index precedes all locally defined functions.
    pub fn add_import(&mut self, module: &'static str, fname: &'static str, type_idx: u32) -> u32 {
        self.imports.push(ImportedFn {
            module,
            name: fname,
            type_idx,
        });
        (self.imports.len() - 1) as u32
    }

    pub fn imported_func_count(&self) -> u32 {
        self.imports.len() as u32
    }

    /// Import a mutable guest register global shared with the execution host.
    pub fn add_imported_global(&mut self, import_name: &'static str) -> u32 {
        self.globals.push((import_name, super::opcodes::I32));
        (self.globals.len() - 1) as u32
    }

    /// Per-instance host-independent state, separate from the guest register ABI.
    pub fn add_private_i32_global(&mut self, initial: i32) -> u32 {
        let index = self.globals.len() + self.private_globals.len();
        self.private_globals.push(initial);
        index as u32
    }

    pub fn add_func(&mut self, def: FuncDef) -> u32 {
        let idx = self.funcs.len() as u32 + self.imports.len() as u32;
        self.funcs.push(def);
        idx
    }

    pub fn name_func(&mut self, index: u32, name: String) {
        self.names.insert(index, name);
    }

    pub fn add_imported_table(&mut self, min: u32) {
        self.table_min = Some(min);
    }

    /// Active element segment: fill the imported table at `offset` with function indices.
    pub fn add_element_segment(&mut self, offset: u32, func_idxs: Vec<u32>) {
        self.segments.push(Segment { offset, func_idxs });
    }

    /// Import (rather than define) the guest memory.
    pub fn import_memory(&mut self, min: u32, max: Option<u32>) {
        self.memory = Some((min, max));
        self.memory_imported = true;
    }

    pub fn export_func(&mut self, name: &'static str, func_idx: u32) {
        self.exports.push(Export {
            name,
            idx: func_idx,
            kind: ExportKind::Func,
        });
    }

    pub fn export_global(&mut self, name: &'static str, index: u32) {
        self.exports.push(Export {
            name,
            idx: index,
            kind: ExportKind::Global,
        });
    }

    pub fn build(&mut self) -> Vec<u8> {
        let mut module = Module::new();
        self.encode_types(&mut module);
        self.encode_imports(&mut module);
        self.encode_functions(&mut module);
        self.encode_memory(&mut module);
        if !self.private_globals.is_empty() {
            let mut section = GlobalSection::new();
            for initial in &self.private_globals {
                section.global(
                    GlobalType {
                        val_type: wasm_encoder::ValType::I32,
                        mutable: true,
                        shared: false,
                    },
                    &ConstExpr::i32_const(*initial),
                );
            }
            module.section(&section);
        }
        self.encode_exports(&mut module);
        self.encode_elements(&mut module);
        let mut suffix = Vec::new();
        if !self.names.is_empty() {
            let mut names = NameMap::new();
            for (&index, name) in &self.names {
                names.append(index, name);
            }
            let mut section = NameSection::new();
            section.functions(&names);
            suffix.push(section.id());
            section.encode(&mut suffix);
        }
        self.encode_code(&mut module, suffix.len());
        let mut bytes = module.finish();
        bytes.extend_from_slice(&suffix);
        // V8 reports module-absolute offsets, beginning after each body's size prefix.
        self.func_body_starts = Parser::new(0)
            .parse_all(&bytes)
            .filter_map(|payload| {
                match payload.expect("encoder produced malformed module framing") {
                    Payload::CodeSectionEntry(body) => Some(body.range().start),
                    _ => None,
                }
            })
            .collect();
        bytes
    }

    fn encode_types(&self, module: &mut Module) {
        if self.types.is_empty() {
            return;
        }
        let mut section = TypeSection::new();
        for (params, results) in &self.types {
            section.ty().function(
                params.iter().copied().map(value_type),
                results.iter().copied().map(value_type),
            );
        }
        module.section(&section);
    }

    fn encode_imports(&self, module: &mut Module) {
        let mut section = ImportSection::new();
        for import in &self.imports {
            section.import(
                import.module,
                import.name,
                EntityType::Function(import.type_idx),
            );
        }
        if let Some(minimum) = self.table_min {
            section.import(
                "env",
                "table",
                TableType {
                    element_type: RefType::FUNCREF,
                    table64: false,
                    minimum: minimum.into(),
                    maximum: None,
                    shared: false,
                },
            );
        }
        if self.memory_imported {
            if let Some((min, max)) = self.memory {
                section.import("env", "memory", memory_type(min, max));
            }
        }
        for (name, ty) in &self.globals {
            section.import(
                "env",
                name,
                GlobalType {
                    val_type: value_type(*ty),
                    mutable: true,
                    shared: false,
                },
            );
        }
        if !section.is_empty() {
            module.section(&section);
        }
    }

    fn encode_functions(&self, module: &mut Module) {
        if self.funcs.is_empty() {
            return;
        }
        let mut section = FunctionSection::new();
        for function in &self.funcs {
            section.function(function.type_idx);
        }
        module.section(&section);
    }

    fn encode_memory(&self, module: &mut Module) {
        if let (Some((min, max)), false) = (self.memory, self.memory_imported) {
            let mut section = MemorySection::new();
            section.memory(memory_type(min, max));
            module.section(&section);
        }
    }

    fn encode_exports(&self, module: &mut Module) {
        if self.exports.is_empty() {
            return;
        }
        let mut section = ExportSection::new();
        for export in &self.exports {
            section.export(export.name, export.kind, export.idx);
        }
        module.section(&section);
    }

    fn encode_elements(&self, module: &mut Module) {
        if self.segments.is_empty() {
            return;
        }
        let mut section = ElementSection::new();
        for segment in &self.segments {
            section.active(
                None,
                &ConstExpr::i32_const(segment.offset as i32),
                Elements::Functions(Cow::Borrowed(&segment.func_idxs)),
            );
        }
        module.section(&section);
    }

    fn encode_code(&self, module: &mut Module, suffix_length: usize) {
        if self.funcs.is_empty() {
            return;
        }
        module.section(&StreamingCode(&self.funcs, suffix_length));
    }
}

fn memory_type(minimum: u32, maximum: Option<u32>) -> MemoryType {
    MemoryType {
        minimum: minimum.into(),
        maximum: maximum.map(u64::from),
        memory64: false,
        shared: false,
        page_size_log2: None,
    }
}

/// The instruction emitter currently uses binary numeric type tags.
fn value_type(ty: ValType) -> wasm_encoder::ValType {
    match ty {
        0x7f => wasm_encoder::ValType::I32,
        0x7e => wasm_encoder::ValType::I64,
        0x7d => wasm_encoder::ValType::F32,
        0x7c => wasm_encoder::ValType::F64,
        _ => panic!("unsupported compiler value type: {ty:#x}"),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/wasm/builder.rs"]
mod tests;
