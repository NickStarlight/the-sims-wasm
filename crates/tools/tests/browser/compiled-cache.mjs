import assert from 'node:assert/strict'
import { openCompiledCache } from '../../../../web/compiled-cache.js'

// A transactional IDB model: requests succeed before commit, aborted transactions
// discard their writes, and all handles share a serialized transaction queue.
function indexedDatabase() {
  const entries = new Map(),
    names = [],
    handles = []
  let created = false,
    fault,
    tail = Promise.resolve()
  const factory = {
    open(name, version) {
      names.push(name)
      assert.equal(version, 1)
      if (fault === 'open-throw') throw new DOMException('Denied', 'SecurityError')
      const request = {}
      setTimeout(() => {
        if (fault === 'open-error') return request.onerror?.()
        const db = {
          closed: false,
          objectStoreNames: { contains: (name) => created && name === 'artifacts' },
          createObjectStore(name) {
            assert.equal(name, 'artifacts')
            if (fault === 'upgrade') throw new Error('Upgrade failed')
            created = true
          },
          close() {
            db.closed = true
          },
          transaction(name, mode) {
            assert.equal(name, 'artifacts')
            assert.ok(['readonly', 'readwrite'].includes(mode))
            if (db.closed || fault === 'transaction') throw new Error('Transaction unavailable')
            const tx = { aborted: false, requests: [] }
            let snapshot
            tx.abort = () => {
              tx.aborted = true
            }
            const request = (kind, key, value) => {
              if (fault === 'clone' && kind === 'put')
                throw new DOMException('Not cloneable', 'DataCloneError')
              const req = {}
              // IndexedDB clones at the call boundary, not when commit later runs.
              const copied = value === undefined ? undefined : structuredClone(value)
              tx.requests.push(() => {
                if (fault === kind) {
                  req.error = new DOMException('Storage failed', 'QuotaExceededError')
                  req.onerror?.()
                  tx.aborted = true
                  tx.onerror?.()
                  return
                }
                if (kind === 'get') req.result = structuredClone(snapshot.get(key))
                if (kind === 'put') snapshot.set(key, copied)
                if (kind === 'delete') snapshot.delete(key)
                req.onsuccess?.()
              })
              return req
            }
            tx.objectStore = (name) => {
              assert.equal(name, 'artifacts')
              return {
                get: (key) => request('get', key),
                put: (value, key) => request('put', key, value),
                delete: (key) => request('delete', key),
              }
            }
            tail = tail.then(async () => {
              await new Promise((resolve) => setTimeout(resolve, 0))
              snapshot = structuredClone(entries)
              while (tx.requests.length && !tx.aborted) tx.requests.shift()()
              if (fault === 'commit' && mode === 'readwrite') tx.aborted = true
              if (tx.aborted) return tx.onabort?.()
              if (mode === 'readwrite') {
                entries.clear()
                for (const [key, value] of snapshot) entries.set(key, value)
              }
              tx.oncomplete?.()
            })
            return tx
          },
        }
        handles.push(db)
        request.result = db
        if (fault === 'blocked') request.onblocked?.()
        if (!created) request.onupgradeneeded?.()
        request.onsuccess?.()
      }, 0)
      return request
    },
  }
  return {
    factory,
    entries,
    names,
    handles,
    fault: (value) => {
      fault = value
    },
  }
}

const value = () => ({
  version: 1,
  wasm: Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0, 0, 2, 1, 120),
  imageBytes: Uint8Array.from({ length: 16 }, (_, i) => i),
  metadata: {
    funcAddrs: [0x1010],
    funcCount: 1,
    tableAddr: 0xa00000,
    trampBase: 32,
    entry: 0x1010,
    commandLineAddr: 0xd00000,
  },
  plan: {
    images: [
      {
        base: 0x1000,
        preferredBase: 0x1000,
        sizeOfImage: 0x1000,
        name: 'Sims.exe',
        path: 'C:\\Sims\\Sims.exe',
        entry: 0x1010,
        isDll: false,
        imports: [{ dll: 'kernel32.dll', name: 'GetVersion', ordinal: null, iatVa: 0x1004 }],
        exports: [],
      },
    ],
    spans: [{ address: 0x1000, offset: 0, length: 16 }],
  },
  clientId: 'a'.repeat(64),
  commandLine: 'C:\\Sims\\Sims.exe -skip_intro',
  initialRegistry: {
    'hkey_local_machine\\software\\maxis\\the sims': { SIMS_GAME_EDITION: { t: 1, d: 'MjU1AA==' } },
  },
})
const key = (change = {}) =>
  JSON.stringify({
    version: 1,
    compilerId: 'b'.repeat(64),
    inputs: [{ path: 'Sims.exe', sha256: 'a'.repeat(64) }],
    commandLine: value().commandLine,
    ...change,
  })
const original = Object.getOwnPropertyDescriptor(globalThis, 'indexedDB')
const install = (factory) =>
  Object.defineProperty(globalThis, 'indexedDB', { configurable: true, value: factory })
let cache,
  count = 0
try {
  install(undefined)
  cache = await openCompiledCache()
  assert.equal(await cache.get(key()), undefined)
  assert.equal(await cache.put(key(), value()), false)
  assert.equal(await cache.delete(key()), false)
  cache.close()
  count++

  for (const fault of ['open-throw', 'open-error', 'blocked', 'upgrade']) {
    const model = indexedDatabase()
    model.fault(fault)
    install(model.factory)
    cache = await openCompiledCache()
    assert.equal(await cache.put(key(), value()), false, fault)
    assert.equal(await cache.get(key()), undefined, fault)
    assert.ok(
      model.handles.every((handle) => handle.closed),
      'late/failed open handles close',
    )
    cache.close()
    count++
  }

  const model = indexedDatabase()
  install(model.factory)
  cache = await openCompiledCache()
  const { get, put, delete: remove } = cache
  assert.equal(await get(key()), undefined)
  const first = value()
  first.files = [new File(['owned executable'], 'Sims.exe')]
  first.metadata.diagnostics = ['must not be retained']
  assert.equal(await put(key(), first), true)
  first.wasm[11] = 121
  const restored = await get(key())
  assert.deepEqual(
    restored,
    value(),
    'cache snapshots buffers and stores only lean execution fields',
  )
  restored.imageBytes[0] = 99
  assert.deepEqual(await get(key()), value(), 'get returns an independent structured clone')
  cache.close()
  cache = await openCompiledCache()
  assert.deepEqual(await cache.get(key()), value(), 'warm reopen restores compiled artifact')
  count++

  const partial = value()
  const wasmBacking = new Uint8Array(partial.wasm.length + 7).fill(99)
  wasmBacking.set(partial.wasm, 3)
  partial.wasm = wasmBacking.subarray(3, 3 + partial.wasm.length)
  const imageBacking = new Uint8Array(64).fill(99)
  imageBacking.set(partial.imageBytes, 8)
  partial.imageBytes = imageBacking.subarray(8, 24)
  assert.equal(await cache.put(key(), partial), true)
  const stored = model.entries.get('current').value
  assert.equal(stored.wasm.buffer.byteLength, partial.wasm.length)
  assert.equal(stored.imageBytes.buffer.byteLength, partial.imageBytes.length)
  assert.deepEqual(
    await cache.get(key()),
    value(),
    'bytes outside typed-array views are not cached',
  )
  count++

  const nullable = value()
  Object.assign(nullable.plan.images[0], {
    name: null,
    entry: null,
    isDll: true,
    exports: [
      { name: null, ordinal: 1, target: { kind: 'address', address: 0x1010 } },
      {
        name: 'forward',
        ordinal: 2,
        target: { kind: 'forwardByName', dll: 'kernel32.dll', name: 'GetVersion' },
      },
      {
        name: 'ordinal',
        ordinal: 3,
        target: { kind: 'forwardByOrdinal', dll: 'other.dll', ordinal: 1 },
      },
    ],
  })
  nullable.plan.images[0].imports[0].name = null
  nullable.plan.images[0].imports[0].ordinal = 1
  assert.equal(await cache.put('nullable', nullable), true)
  assert.deepEqual(
    await cache.get('nullable'),
    nullable,
    'legal PE nullable/forwarded metadata survives',
  )
  count++

  const empty = value()
  empty.metadata = {
    funcAddrs: [],
    funcCount: 0,
    entry: 0,
    tableAddr: 0,
    trampBase: 0,
    commandLineAddr: 0,
  }
  empty.plan = { images: [], spans: [] }
  empty.imageBytes = new Uint8Array()
  assert.equal(await cache.put('empty-fixture', empty), true)
  assert.deepEqual(
    await cache.get('empty-fixture'),
    empty,
    'zero-function synthetic compiler output is supported',
  )
  assert.equal(await cache.put(key(), value()), true)
  count++

  let current = key()
  for (const changed of [
    { compilerId: 'c'.repeat(64) },
    { inputs: [{ path: 'Sims.exe', sha256: 'd'.repeat(64) }] },
    {
      inputs: [
        { path: 'Sims.exe', sha256: 'a'.repeat(64) },
        { path: 'ijl10.dll', sha256: 'e'.repeat(64) },
      ],
    },
    { commandLine: 'C:\\Sims\\Sims.exe' },
  ]) {
    const next = key(changed)
    assert.equal(await cache.get(next), undefined)
    assert.equal(await cache.put(next, value()), true)
    assert.equal(await cache.get(current), undefined)
    assert.deepEqual(await cache.get(next), value())
    assert.equal(model.entries.size, 1, 'one current artifact bounds recompilation storage')
    current = next
    count++
  }

  for (const fault of ['put', 'commit', 'clone', 'transaction']) {
    model.fault(fault)
    assert.equal(await cache.put('replacement', value()), false, fault)
    model.fault(undefined)
    assert.deepEqual(
      await cache.get(current),
      value(),
      'failed replacement preserves prior artifact',
    )
    count++
  }
  model.fault('get')
  assert.equal(await cache.get(current), undefined)
  model.fault(undefined)
  assert.deepEqual(await cache.get(current), value())
  count++

  assert.equal(await cache.delete('different-key'), false)
  assert.deepEqual(await cache.get(current), value())
  assert.equal(await cache.delete(current), true)
  assert.equal(await cache.get(current), undefined)
  count++

  for (const corrupt of [
    (record) => {
      record.version = 2
    },
    (record) => {
      record.value.version = 2
    },
    (record) => {
      record.hashes.pop()
    },
    (record) => {
      record.value.wasm[11] ^= 1
    },
    (record) => {
      record.value.imageBytes[0] ^= 1
    },
    (record) => {
      record.value.metadata.entry++
    },
    (record) => {
      record.value.metadata.funcCount++
    },
    (record) => {
      record.value.plan.spans[0].length++
    },
    (record) => {
      record.value.initialRegistry = null
    },
  ]) {
    assert.equal(await cache.put(current, value()), true)
    corrupt(model.entries.get('current'))
    assert.equal(await cache.get(current), undefined, 'corrupt stored artifact is a cache miss')
    count++
  }

  await cache.put(current, value())
  for (const alter of [
    (v) => {
      v.wasm[0] = 1
    },
    (v) => {
      v.wasm = v.wasm.buffer
    },
    (v) => {
      v.imageBytes = new Uint8Array(17)
    },
    (v) => {
      v.plan.spans[0].offset = 1
    },
    (v) => {
      v.plan.spans[0].address = 0xfffffff8
    },
    (v) => {
      v.plan.images[0].imports[0].iatVa = 0
    },
    (v) => {
      v.plan.images[0].exports = [
        { name: 'bad', ordinal: 1, target: { kind: 'address', address: 0 } },
      ]
    },
    (v) => {
      v.plan.file = new File(['raw'], 'Sims.exe')
    },
    (v) => {
      v.clientId = 'missing'
    },
    (v) => {
      v.commandLine += '\0bad'
    },
    (v) => {
      v.initialRegistry.self = v.initialRegistry
    },
  ]) {
    const bad = value()
    alter(bad)
    assert.equal(await cache.put('bad', bad), false)
    assert.deepEqual(
      await cache.get(current),
      value(),
      'invalid candidate cannot evict the working artifact',
    )
    count++
  }
  for (const invalid of [undefined, null, '', 'x'.repeat(65537)]) {
    assert.equal(await cache.get(invalid), undefined)
    assert.equal(await cache.put(invalid, value()), false)
    assert.equal(await cache.delete(invalid), false)
    count++
  }

  const peer = await openCompiledCache()
  await Promise.all([cache.put('next-key', value()), peer.delete(current)])
  assert.deepEqual(
    await peer.get('next-key'),
    value(),
    'deleting an old key cannot remove its replacement',
  )
  peer.close()
  count++
  model.handles.at(-2).onversionchange()
  assert.equal(await cache.get('next-key'), undefined)
  assert.equal(await cache.put(current, value()), false)
  cache.close()
  count++
  cache = await openCompiledCache()
  const pending = cache.put('closed-during-hash', value())
  cache.close()
  assert.equal(await pending, false)
  assert.ok(
    model.names.every((name) => name === 'sims-web-compiled-v1'),
    'no asset/save/folder database is touched',
  )
  count++
  console.log(
    `Compiled artifact cache: ${count} reload/key/eviction/integrity/error/lifecycle cases passed`,
  )
} finally {
  cache?.close()
  if (original) Object.defineProperty(globalThis, 'indexedDB', original)
  else delete globalThis.indexedDB
}
