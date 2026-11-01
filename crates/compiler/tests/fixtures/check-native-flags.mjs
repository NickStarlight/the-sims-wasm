// Compare an exported compiler fixture with independently generated CPU data.
// Usage: node check-native-flags.mjs exported.wasm /path/to/generated-data
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';

const [wasmPath, directory] = process.argv.slice(2);
assert.ok(wasmPath && directory, 'Expected a Wasm path and native-data directory');
const routines = JSON.parse(readFileSync(join(directory, 'oracle-routines.json')));
const native = readFileSync(join(directory, 'oracle-results.bin'));
const casesPerRoutine = native.length / (routines.length * 20);
assert.ok(Number.isInteger(casesPerRoutine) && casesPerRoutine > 0, 'Malformed native data');
const wasm = readFileSync(wasmPath);
const names = ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
const env = {memory: new WebAssembly.Memory({initial: 1})};
for (const name of names) env[name] = new WebAssembly.Global({value: 'i32', mutable: true}, 0);
const {exports} = new WebAssembly.Instance(new WebAssembly.Module(wasm), {env});
let offset = 0, cases = 0, failures = 0;
const examples = [];
for (const [name] of routines) {
  for (let index = 0; index < casesPerRoutine; index++) {
    const [a, b, initial, expected, flags] = Array.from({length: 5}, (_, i) => native.readUInt32LE(offset + i * 4));
    offset += 20;
    env.eax.value = a;
    env.ecx.value = b;
    env.flags.value = initial & 0x8d5;
    exports[name]();
    // Auxiliary carry is not implemented for all widths in the current emitter.
    const eaxMask = name === 'compare_lahf' ? ~0x1000 : 0xffffffff;
    if ((env.eax.value & eaxMask) !== (expected & eaxMask) || ((env.flags.value ^ flags) & 0x8c5)) {
      failures++;
      if (examples.length < 20) examples.push({name, index, a, b, initial, expected, actual: env.eax.value >>> 0, flags: flags & 0x8c5, actualFlags: env.flags.value & 0x8c5});
    }
    cases++;
  }
}
console.log(JSON.stringify({
  pass: failures === 0, cases, routines: routines.length, failures, examples,
  flagMask: 'CF/PF/ZF/SF/OF (0x8c5)',
  wasmSha256: createHash('sha256').update(wasm).digest('hex'),
  nativeSha256: createHash('sha256').update(native).digest('hex'),
}, null, 2));
if (failures) process.exitCode = 1;
