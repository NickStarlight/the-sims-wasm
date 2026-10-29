import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

export async function commandLineSmoke(directory) {
  const { default: init, Compiler } = await import(new URL('./compiler.js', directory));
  await init({ module_or_path: readFileSync(new URL('./compiler_bg.wasm', directory)) });
  const { createMachine, registerNames } = await import(new URL('./machine.js', directory));
  const { createGuestCaller } = await import(new URL('./callbacks.js', directory));
  const compiler = new Compiler(executable());
  let artifact;
  let program;
  try {
    artifact = compiler.compile(Uint32Array.of(0x401020));
    program = {
      metadata: JSON.parse(artifact.metadataJson()),
      plan: JSON.parse(compiler.loadPlanJson()),
      imageBytes: compiler.loadBytes(),
      module: await WebAssembly.compile(artifact.wasm()),
      commandLine: 'C:\\Sims\\Sims.exe -skip_intro',
    };
  } finally { artifact?.free(); compiler.free(); }
  const machine = await createMachine(program, () => {});
  try {
    const { process, memory, table, registers } = machine;
    assert.deepEqual([...process.read(0x700000, 16)], Array(16).fill(0xa5), 'Complete image data survives command-line setup');
    const command = new TextEncoder().encode(`${program.commandLine}\0`);
    const address = program.metadata.commandLineAddr;
    assert.deepEqual(process.read(address, command.length), command);
    const api = JSON.parse(process.resolve('kernel32.dll', 'GetCommandLineA'));
    assert.equal(process.call(api.id, new Uint32Array()), address);
    const instance = new WebAssembly.Instance(program.module, {
      env: { memory, table, ...registers, dispatch() { assert.fail('unexpected dispatch'); }, blockTrace() {} },
    });
    instance.exports.run(program.metadata.entry);
    assert.equal(registers.eax.value, address, 'entry argument and GetCommandLineA use the same buffer');
    new DataView(memory.buffer).setUint16(0x700010, 0x173f, true);
    const originalStack = registers.esp.value;
    let fail = false;
    const callGuest = createGuestCaller(registers, registerNames, memory, {
      async run(runAt, target) {
        runAt(target);
        assert.equal(instance.exports.fpuControl.value, 0x173f, 'compiled callback changes its rounding mode');
        if (fail) throw new Error('callback failure');
      },
    }, instance.exports);
    instance.exports.fpuControl.value = 0x0a7f;
    assert.equal(await callGuest(0x401020, [7]), 42);
    assert.equal(instance.exports.fpuControl.value, 0x0a7f, 'callback restores caller rounding mode');
    assert.equal(registers.esp.value, originalStack);
    fail = true;
    await assert.rejects(callGuest(0x401020, [7]), /callback failure/);
    assert.equal(instance.exports.fpuControl.value, 0x0a7f, 'failed callback restores caller rounding mode');
    assert.equal(registers.esp.value, originalStack);
  } finally { machine.process.free(); }
  await assert.rejects(createMachine({ ...program, commandLine: 'x'.repeat(4096) }, () => {}), /4096 bytes/);
  await assert.rejects(createMachine({ ...program, commandLine: 'Sims.exe\0-skip_intro' }, () => {}), /NUL/);
  const incompleteMetadata = { ...program.metadata };
  delete incompleteMetadata.commandLineAddr;
  await assert.rejects(createMachine({ ...program, metadata: incompleteMetadata }, () => {}), /bundled command-line address/i, 'missing metadata cannot fall back into Complete image data');
  console.log('Command line: compiler, mapped image data, entry argument and GetCommandLineA passed');
}

function executable() {
  const bytes = new Uint8Array(0x600), view = new DataView(bytes.buffer);
  const word = (at, value) => view.setUint16(at, value, true);
  const dword = (at, value) => view.setUint32(at, value, true);
  word(0, 0x5a4d); dword(0x3c, 0x80); dword(0x80, 0x4550);
  word(0x84, 0x14c); word(0x86, 2); word(0x94, 0xe0); word(0x98, 0x10b);
  dword(0xa8, 0x1000); dword(0xb4, 0x400000); dword(0xd0, 0x301000); dword(0xd4, 0x200);
  for (const [section, rva, offset, flags] of [[0x178, 0x1000, 0x200, 0x60000020], [0x1a0, 0x300000, 0x400, 0xc0000040]]) {
    dword(section + 8, 0x200); dword(section + 12, rva); dword(section + 16, 0x200);
    dword(section + 20, offset); dword(section + 36, flags);
  }
  bytes.set([0x8b, 0x44, 0x24, 12, 0xc3], 0x200); // mov eax,[esp+12]; ret
  bytes.set([0xd9, 0x2d, 0x10, 0, 0x70, 0, 0xb8, 42, 0, 0, 0, 0xc2, 4, 0], 0x220); // fldcw [0x700010]; mov eax,42; ret 4
  bytes.fill(0xa5, 0x400);
  return bytes;
}
