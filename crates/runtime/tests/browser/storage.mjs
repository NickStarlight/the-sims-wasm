import assert from 'node:assert/strict'

export async function storageSmoke(Process, directory) {
  const { openSaveStorage } = await import(new URL('./storage.js', directory))
  const clientId = 'a'.repeat(64),
    otherClientId = 'b'.repeat(64)
  const entries = new Map()
  let storageOpens = 0
  const original = Object.getOwnPropertyDescriptor(navigator, 'storage')
  const root = {
    async getDirectoryHandle() {
      return root
    },
    async getFileHandle(name) {
      let entry = entries.get(name)
      if (!entry) entries.set(name, (entry = { bytes: new Uint8Array(), flushed: 0, closed: 0 }))
      return {
        async createSyncAccessHandle() {
          return {
            getSize: () => entry.bytes.length,
            read(target, { at }) {
              const n = Math.min(target.length, 7, entry.bytes.length - at)
              target.set(entry.bytes.subarray(at, at + n))
              return n
            },
            write(source, { at }) {
              const n = Math.min(source.length, 11)
              const bytes = new Uint8Array(Math.max(entry.bytes.length, at + n))
              bytes.set(entry.bytes)
              bytes.set(source.subarray(0, n), at)
              entry.bytes = bytes
              return n
            },
            truncate(size) {
              entry.bytes = entry.bytes.slice(0, size)
            },
            flush() {
              entry.flushed++
            },
            close() {
              entry.closed++
            },
          }
        },
      }
    },
  }
  Object.defineProperty(navigator, 'storage', {
    configurable: true,
    value: {
      getDirectory: async () => {
        storageOpens++
        return root
      },
    },
  })
  const files = [
    { path: 'UserData/Neighborhood.iff', file: new File(['neighborhood'], 'Neighborhood.iff') },
    {
      path: 'Sims.exe',
      file: {
        arrayBuffer() {
          throw new Error('Selected executable must not identify save storage')
        },
      },
    },
  ]
  const make = () => {
    const process = new Process(new WebAssembly.Memory({ initial: 2 }), 0x10000, 0x100000)
    process.adopt(0x1000, 0x1000, true)
    process.addFile('UserData/options.iff', Uint8Array.of(1, 2, 3))
    process.write(0x1000, new TextEncoder().encode('UserData/options.iff\0'))
    return process
  }
  const call = (p, name, args) =>
    p.call(JSON.parse(p.resolve('kernel32.dll', name)).id, Uint32Array.from(args))
  let process, storage
  try {
    process = make()
    for (const invalid of [undefined, null, '', 'same-client', '../journal', 'A'.repeat(64)]) {
      await assert.rejects(
        openSaveStorage(files, process, invalid),
        /Invalid game identity/,
      )
    }
    assert.equal(storageOpens, 0, 'invalid identities fail before accessing save storage')
    storage = await openSaveStorage(files, process, clientId)
    const handle = call(process, 'CreateFileA', [0x1000, 0xc0000000, 3, 0, 3, 0, 0])
    process.write(0x1100, Uint8Array.of(9, 8, 7))
    assert.equal(call(process, 'WriteFile', [handle, 0x1100, 3, 0x1200, 0]), 1)
    storage.flush()
    await storage.close()
    process.free()
    process = make()
    storage = await openSaveStorage(files, process, clientId)
    const read = call(process, 'CreateFileA', [0x1000, 0x80000000, 3, 0, 3, 0, 0])
    assert.equal(call(process, 'ReadFile', [read, 0x1100, 3, 0x1200, 0]), 1)
    assert.deepEqual([...process.read(0x1100, 3)], [9, 8, 7])
    assert.equal(entries.size, 1, 'same source/client must restore the same journal')
    const neighborhoodHash = Buffer.from(
      await crypto.subtle.digest('SHA-256', new TextEncoder().encode('neighborhood')),
    ).toString('hex')
    assert.deepEqual(
      [...entries.keys()],
      [`${clientId}-${neighborhoodHash}.journal`],
      'existing client-hash/neighborhood-hash namespace is unchanged',
    )
    await storage.close()
    process.free()
    process = make()
    storage = await openSaveStorage(files, process, otherClientId)
    const other = call(process, 'CreateFileA', [0x1000, 0x80000000, 3, 0, 3, 0, 0])
    assert.equal(call(process, 'ReadFile', [other, 0x1100, 3, 0x1200, 0]), 1)
    assert.deepEqual(
      [...process.read(0x1100, 3)],
      [1, 2, 3],
      "different bundled identities cannot inherit each other's file saves",
    )
    assert.equal(entries.size, 2)
  } finally {
    await storage?.close()
    process?.free()
    if (original) Object.defineProperty(navigator, 'storage', original)
    else delete navigator.storage
  }
  console.log('Browser save journal reload: passed')
}

export async function resetSmoke(directory) {
  const { openSaveStorage, resetProgress, loadRegistry, storeRegistry } = await import(
    new URL('./storage.js', directory)
  )
  const originalStorage = Object.getOwnPropertyDescriptor(navigator, 'storage')
  const originalLocal = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
  const registry = new Map([
    ['sims-web:registry-v2', 'saved preferences'],
    ['unrelated', 'keep'],
  ])
  const entries = new Set(['sims-web-saves-v1', 'unrelated-data'])
  let removalError,
    openingError,
    removals = 0,
    closed = 0
  const root = {
    async removeEntry(name, options) {
      assert.equal(name, 'sims-web-saves-v1')
      assert.deepEqual(options, { recursive: true })
      if (removalError) throw removalError
      if (!entries.delete(name)) throw new DOMException('Missing', 'NotFoundError')
      removals++
    },
    async getDirectoryHandle() {
      if (openingError) throw openingError
      return root
    },
    async getFileHandle() {
      return {
        async createSyncAccessHandle() {
          return {
            getSize: () => 0,
            close() {
              closed++
            },
          }
        },
      }
    },
  }
  Object.defineProperty(navigator, 'storage', {
    configurable: true,
    value: { getDirectory: async () => root },
  })
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      get length() {
        return registry.size
      },
      key: (index) => [...registry.keys()][index] ?? null,
      getItem: (key) => registry.get(key) ?? null,
      setItem: (key, value) => registry.set(key, value),
      removeItem: (key) => registry.delete(key),
    },
  })
  let storage
  try {
    const clientId = 'a'.repeat(64),
      otherClientId = 'b'.repeat(64)
    for (const invalid of [undefined, null, '', 'complete', 'A'.repeat(64)]) {
      assert.throws(() => loadRegistry(invalid), /Invalid game identity/)
      assert.throws(
        () => storeRegistry('invalid preferences', invalid),
        /Invalid game identity/,
      )
    }
    assert.equal(loadRegistry(clientId), null, 'unscoped registry data is not inherited')
    storeRegistry('complete preferences', clientId)
    storeRegistry('other preferences', otherClientId)
    assert.equal(loadRegistry(clientId), 'complete preferences')
    assert.equal(loadRegistry(otherClientId), 'other preferences')
    assert.equal(
      registry.get('sims-web:registry-v2'),
      'saved preferences',
      'unscoped data stays untouched',
    )
    const process = { restoreJournal: () => 0 }
    storage = await openSaveStorage([], process, clientId)
    await assert.rejects(resetProgress(), /Close other running game tabs/)
    assert.equal(removals, 0, 'active session must block reset before deletion')
    assert.equal(registry.get(`sims-web:registry-v2:${clientId}`), 'complete preferences')
    await storage.close()
    storage = undefined
    assert.equal(closed, 1)

    removalError = new DOMException('Save is locked', 'NoModificationAllowedError')
    await assert.rejects(resetProgress(), /Save is locked/)
    assert.equal(
      registry.get(`sims-web:registry-v2:${clientId}`),
      'complete preferences',
      'failed deletion must not clear registry',
    )
    removalError = undefined
    await resetProgress()
    assert.deepEqual([...entries], ['unrelated-data'])
    assert.deepEqual(
      [...registry],
      [
        ['sims-web:registry-v2', 'saved preferences'],
        ['unrelated', 'keep'],
      ],
      'reset only removes current scoped registry keys',
    )
    await resetProgress() // no existing saves is a successful reset
    assert.equal(removals, 1)

    openingError = new Error('Could not open save')
    await assert.rejects(openSaveStorage([], process, clientId), /Could not open save/)
    await resetProgress() // failed startup released its shared lease
  } finally {
    await storage?.close()
    if (originalStorage) Object.defineProperty(navigator, 'storage', originalStorage)
    else delete navigator.storage
    if (originalLocal) Object.defineProperty(globalThis, 'localStorage', originalLocal)
    else delete globalThis.localStorage
  }
  console.log(
    'Reset progress: scoped deletion, busy sessions, errors, empty saves and lock cleanup passed',
  )
}
