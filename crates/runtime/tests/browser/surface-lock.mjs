import assert from 'node:assert/strict'
import { surfaceFixture } from '../support/surfaces.mjs'

export function surfaceLockSmoke(Process) {
  const f = surfaceFixture(Process, 4, 2)
  const held = []
  try {
    const source = f.surface(false)
    const destination = f.surface(true)
    const pointer = f.lock(source)
    const values = []
    f.unlock(source)
    for (let i = 0; i < 24; i++) {
      assert.equal(f.lock(source), pointer)
      const expected = Uint8Array.from({ length: 16 }, (_, j) => (i * 17 + j * 29) & 255)
      f.owner.write(pointer, expected)
      f.unlock(source)
      assert.equal(f.call('Surface_Blt', [destination, 0, source, 0, 0, 0]), 0)
      if (i % 4 === 0) {
        held.push(f.owner.takePackedFrame())
        values.push(expected)
      }
      const destinationPointer = f.lock(destination)
      assert.deepEqual(f.owner.read(destinationPointer, 16), expected)
      f.unlock(destination)
      for (let j = 0; j < held.length; j++) assert.deepEqual(held[j].pixels(), values[j])
    }
    f.word(512, 1)
    f.word(516, 0)
    f.word(520, 2)
    f.word(524, 1)
    const destinationPointer = f.lock(destination, f.scratch + 512, 0x10)
    assert.equal(f.read(268), 4, 'legacy subrect lock retains complete width')
    const blue = Uint8Array.from({ length: 16 }, (_, i) => (i % 2 ? 0 : 31))
    f.owner.write(destinationPointer, blue)
    f.unlock(destination)
    const rgba = f.owner.takeFrame()
    assert.deepEqual(
      rgba.pixels(),
      Uint8Array.from({ length: 32 }, (_, i) => (i % 4 >= 2 ? 255 : 0)),
    )
    const rgbaCopy = rgba.pixels()
    const locked = f.lock(destination)
    f.owner.write(locked, new Uint8Array(16))
    f.unlock(destination)
    const latest = f.owner.takePackedFrame()
    assert.deepEqual(latest.pixels(), new Uint8Array(16))
    assert.deepEqual(rgba.pixels(), rgbaCopy)
    rgba.free()
    latest.free()
    assert.equal(f.call('Surface_Release', [source]), 0)
    assert.equal(f.call('Surface_Release', [destination]), 0)
    for (let j = 0; j < held.length; j++) assert.deepEqual(held[j].pixels(), values[j])
    console.log(
      'Surface synchronization: repeated full imports, shared blits, held packed/RGBA frames, legacy subrects and release passed',
    )
  } finally {
    held.forEach((frame) => frame.free())
    f.owner.free()
  }
}
