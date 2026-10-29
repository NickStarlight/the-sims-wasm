use super::*;

#[test]
fn preserves_guest_imports_elements_and_debug_offsets() {
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.add_import("env", "shim", signature);
    builder.add_imported_table(4);
    builder.import_memory(1, Some(2));
    builder.add_imported_global("eax");
    let function = builder.add_func(FuncDef {
        type_idx: signature,
        locals: vec![0x7f, 0x7f, 0x7c],
        body: vec![0x01],
    });
    builder.add_element_segment(1, vec![function]);
    builder.export_func("run", function);

    // Captured from the previous serializer: protects byte layout and debugger ABI.
    let expected = [
        0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 2, 52, 4, 3, 101, 110, 118, 4, 115, 104,
        105, 109, 0, 0, 3, 101, 110, 118, 5, 116, 97, 98, 108, 101, 1, 112, 0, 4, 3, 101, 110, 118,
        6, 109, 101, 109, 111, 114, 121, 2, 1, 1, 2, 3, 101, 110, 118, 3, 101, 97, 120, 3, 127, 1,
        3, 2, 1, 0, 7, 7, 1, 3, 114, 117, 110, 0, 1, 9, 7, 1, 0, 65, 1, 11, 1, 1, 10, 9, 1, 7, 2,
        2, 127, 1, 124, 1, 11,
    ];
    let bytes = builder.build();
    assert_eq!(bytes, expected);
    assert_eq!(builder.func_body_starts, [94]);
    wasmparser::validate(&bytes).unwrap();
    let repeated = builder.build();
    assert_eq!(
        repeated, expected,
        "streaming build preserves reusable builder semantics"
    );
    assert_eq!(builder.func_body_starts, [94]);
}

#[test]
fn streams_code_identically_to_reference_encoder_across_leb_boundaries() {
    let lengths = [0, 1, 126, 127, 128, 16383, 16384];
    let definitions: Vec<_> = (0..129)
        .map(|index| FuncDef {
            type_idx: 0,
            locals: if index % 2 == 0 {
                vec![0x7f; 129]
            } else {
                vec![0x7f, 0x7f, 0x7c]
            },
            body: vec![0x01; lengths[index % lengths.len()]],
        })
        .collect();
    let mut reference = wasm_encoder::CodeSection::new();
    for definition in &definitions {
        let mut function =
            Function::new_with_locals_types(definition.locals.iter().copied().map(value_type));
        function.raw(definition.body.iter().copied());
        function.instruction(&wasm_encoder::Instruction::End);
        reference.function(&function);
    }
    let mut expected = Vec::new();
    reference.encode(&mut expected);
    let mut actual = Vec::new();
    StreamingCode(&definitions, 0).encode(&mut actual);
    assert_eq!(actual, expected);
}
