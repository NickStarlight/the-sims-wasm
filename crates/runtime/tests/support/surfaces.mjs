import assert from 'node:assert/strict'

export function surfaceFixture(Process, width = 800, height = 600) {
  const memory = new WebAssembly.Memory({ initial: 64, maximum: 512 })
  const owner = new Process(memory, 0x10000, 0x2000000)
  const methods = JSON.parse(owner.imports()).filter((a) => a.dll === 'ddraw.dll')
  const ids = Object.fromEntries(methods.map((a) => [a.name, a.id]))
  owner.installModules(
    JSON.stringify({
      images: [{ base: 0x400000, sizeOfImage: 4096, isDll: false }],
      nativeDlls: ['ddraw.dll'],
      nativeBindings: methods.map((a) => ({
        dll: a.dll,
        name: a.name,
        address: 0x800000 + a.id * 4,
      })),
    }),
  )
  const call = (name, args) => owner.call(ids[name], Uint32Array.from(args))
  const scratch = owner.heapAlloc(1024)
  const word = (offset, value) =>
    new DataView(memory.buffer).setUint32(scratch + offset, value, true)
  const read = (offset) => new DataView(memory.buffer).getUint32(scratch + offset, true)
  assert.equal(call('DirectDrawCreate', [0, scratch + 128, 0]), 0)
  const device = read(128)
  const surface = (primary = false, bits = 16) => {
    owner.write(scratch, new Uint8Array(108))
    for (const [offset, value] of [
      [0, 108],
      [4, 0x1007],
      [8, height],
      [12, width],
      [72, 32],
      [76, bits === 8 ? 0x60 : 0x40],
      [84, bits],
      [104, primary ? 0x200 : 0],
    ])
      word(offset, value)
    assert.equal(call('DDraw_CreateSurface', [device, scratch, scratch + 128, 0]), 0)
    return read(128)
  }
  const lock = (id, rect = 0, flags = 0) => {
    word(256, 108)
    assert.equal(call('Surface_Lock', [id, rect, scratch + 256, flags, 0]), 0)
    return read(292)
  }
  const unlock = (id) => assert.equal(call('Surface_Unlock', [id, 0]), 0)
  return { memory, owner, ids, scratch, width, height, word, read, call, surface, lock, unlock }
}
