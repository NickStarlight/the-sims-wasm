import assert from 'node:assert/strict';

export async function checksumSmoke(directory) {
  const { checksum } = await import(new URL('./asset-cache.js', directory));
  const reference = bytes => {
    let crc = 0xffffffff;
    for (const byte of bytes) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    return (crc ^ 0xffffffff) >>> 0;
  };
  assert.equal(checksum(new TextEncoder().encode('123456789')), 0xcbf43926);
  const bytes = Uint8Array.from({ length: 65536 + 31 }, (_, i) => (i * 73 + (i >>> 7)) & 255);
  for (let offset = 0; offset < 16; offset++) {
    for (let length = 0; length <= 513; length++) {
      const view = bytes.subarray(offset, offset + length);
      assert.equal(checksum(view), reference(view), `length${length} offset${offset}`);
    }
    const page = bytes.subarray(offset, offset + 65536);
    assert.equal(checksum(page), reference(page), `64KiB offset${offset}`);
  }
  for (const value of [0, 255]) {
    const page = new Uint8Array(65536).fill(value);
    assert.equal(checksum(page), reference(page));
  }
  assert.equal(checksum(Uint8Array.from({ length: 256 }, (_, i) => i)), reference(Uint8Array.from({ length: 256 }, (_, i) => i)));
  console.log('Asset CRC32: independent bitwise oracle, persisted standard vector, all short tails/offsets and full cache pages passed');
}
