// Optional local compilation cache. One transactional slot replaces the previous
// compiler/input/options key; save journals and asset pages use separate storage.
const databaseName = 'sims-web-compiled-v1'
const storeName = 'artifacts'
const slot = 'current'
const maximumWasm = 256 * 1024 * 1024
const maximumImages = 128 * 1024 * 1024
const encoder = new TextEncoder()
const hashPattern = /^[0-9a-f]{64}$/
const uint = (value) => Number.isInteger(value) && value >= 0 && value <= 0xffffffff
const object = (value) =>
  value !== null && typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype
const keyValid = (key) => typeof key === 'string' && key.length > 0 && key.length <= 65536
const text = (value) => typeof value === 'string' && value.length <= 65536
const range = (address, length) => uint(address) && uint(length) && address + length <= 2 ** 32

function jsonValue(value, seen = new Set(), depth = 0) {
  if (depth > 32) return false

  if (value === null || typeof value === 'boolean' || typeof value === 'string') return true

  if (typeof value === 'number') return Number.isFinite(value)

  if ((!Array.isArray(value) && !object(value)) || seen.has(value)) return false

  seen.add(value)
  const valid = Object.values(value).every((child) => jsonValue(child, seen, depth + 1))
  seen.delete(value)

  return valid
}

function payload(value) {
  if (!object(value) || value.version !== 1) return

  const { wasm, imageBytes, metadata: m, plan, clientId, commandLine, initialRegistry } = value

  if (
    !(wasm instanceof Uint8Array) ||
    wasm.length < 8 ||
    wasm.length > maximumWasm ||
    ![0, 97, 115, 109, 1, 0, 0, 0].every((byte, index) => wasm[index] === byte) ||
    !(imageBytes instanceof Uint8Array) ||
    imageBytes.length > maximumImages ||
    !object(m) ||
    !Array.isArray(m.funcAddrs) ||
    m.funcAddrs.length > 1000000 ||
    m.funcCount !== m.funcAddrs.length ||
    !m.funcAddrs.every(uint) ||
    ![m.tableAddr, m.trampBase, m.entry, m.commandLineAddr].every(uint) ||
    !object(plan) ||
    !Array.isArray(plan.images) ||
    !Array.isArray(plan.spans) ||
    plan.images.length > 256 ||
    plan.spans.length > 100000 ||
    typeof clientId !== 'string' ||
    !hashPattern.test(clientId) ||
    typeof commandLine !== 'string' ||
    commandLine.includes('\0') ||
    encoder.encode(commandLine).length >= 4096 ||
    !object(initialRegistry)
  )
    return

  for (const image of plan.images) {
    if (
      !object(image) ||
      !range(image.base, image.sizeOfImage) ||
      !(image.entry === null || uint(image.entry)) ||
      !uint(image.preferredBase) ||
      !(image.name === null || text(image.name)) ||
      !text(image.path) ||
      typeof image.isDll !== 'boolean' ||
      !Array.isArray(image.imports) ||
      !Array.isArray(image.exports) ||
      image.imports.length > 16384 ||
      image.exports.length > 100000
    )
      return

    for (const entry of image.imports) {
      if (
        !object(entry) ||
        !text(entry.dll) ||
        !uint(entry.iatVa) ||
        !(text(entry.name) || (entry.name === null && uint(entry.ordinal))) ||
        entry.iatVa < image.base ||
        entry.iatVa + 4 > image.base + image.sizeOfImage
      )
        return
    }

    for (const entry of image.exports) {
      if (!object(entry) || !uint(entry.ordinal) || !(entry.name === null || text(entry.name)))
        return

      const target = entry.target

      if (!object(target)) return

      if (target.kind === 'address') {
        if (
          !uint(target.address) ||
          target.address < image.base ||
          target.address >= image.base + image.sizeOfImage
        )
          return
      } else if (target.kind === 'forwardByName') {
        if (!text(target.dll) || !text(target.name)) return
      } else if (target.kind === 'forwardByOrdinal') {
        if (!text(target.dll) || !uint(target.ordinal)) return
      } else return
    }
  }

  let packed = 0

  for (const span of plan.spans) {
    if (
      !object(span) ||
      !range(span.address, span.length) ||
      span.offset !== packed ||
      span.length === 0 ||
      packed + span.length > imageBytes.length ||
      !plan.images.some(
        (image) =>
          span.address >= image.base &&
          span.address + span.length <= image.base + image.sizeOfImage,
      )
    )
      return

    packed += span.length
  }

  if (packed !== imageBytes.length) return

  // Only execution metadata belongs here: no diagnostics, selected File objects
  // or appended raw executable files. The loader reattaches owned files each time.
  const metadata = {
    funcAddrs: m.funcAddrs,
    funcCount: m.funcCount,
    tableAddr: m.tableAddr,
    trampBase: m.trampBase,
    entry: m.entry,
    commandLineAddr: m.commandLineAddr,
  }
  const header = { version: 1, metadata, plan, clientId, commandLine, initialRegistry }

  if (!jsonValue(header)) return

  const serialized = encoder.encode(JSON.stringify(header))

  if (serialized.length > 8 * 1024 * 1024) return

  // IndexedDB clones a typed array's entire backing buffer, including bytes
  // outside its view. Compact views so unrelated raw/compiler memory stays out.
  const compact = (bytes) =>
    bytes.byteOffset || bytes.byteLength !== bytes.buffer.byteLength ? bytes.slice() : bytes

  return { value: { ...header, wasm: compact(wasm), imageBytes: compact(imageBytes) }, serialized }
}

async function hashes({ value, serialized }) {
  return Promise.all(
    [serialized, value.wasm, value.imageBytes].map(async (bytes) =>
      [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
        .map((byte) => byte.toString(16).padStart(2, '0'))
        .join(''),
    ),
  )
}

function transact(database, mode, operation) {
  return new Promise((resolve) => {
    let transaction
    let result

    try {
      transaction = database.transaction(storeName, mode)
      transaction.oncomplete = () => resolve(result)
      transaction.onabort = transaction.onerror = () => resolve(undefined)
      operation(transaction.objectStore(storeName), (value) => (result = value))
    } catch {
      try {
        transaction?.abort()
      } catch {
        // The transaction may already have finished.
      }

      resolve(undefined)
    }
  })
}

export async function openCompiledCache() {
  const unavailable = {
    get: async () => undefined,
    put: async () => false,
    delete: async () => false,
    close() {},
  }
  let database

  try {
    if (typeof indexedDB === 'undefined' || !globalThis.crypto?.subtle) return unavailable

    database = await new Promise((resolve) => {
      const request = indexedDB.open(databaseName, 1)
      let settled = false

      const finish = (value) => {
        if (settled) return value?.close()

        settled = true
        clearTimeout(timer)
        resolve(value)
      }

      // A blocked upgrade or unavailable storage must not prevent local compilation.
      const timer = setTimeout(() => finish(), 2000)
      request.onblocked = request.onerror = () => finish()

      request.onupgradeneeded = () => {
        try {
          if (!request.result.objectStoreNames.contains(storeName))
            request.result.createObjectStore(storeName)
        } catch {
          try {
            request.transaction?.abort()
          } catch {
            // A failed upgrade may already have aborted its transaction.
          }

          finish()
        }
      }

      request.onsuccess = () => finish(request.result)
    })
  } catch {
    return unavailable
  }

  if (!database) return unavailable

  let closed = false

  const close = () => {
    closed = true
    database.close()
  }

  database.onversionchange = close
  database.onclose = () => (closed = true)

  return {
    async get(key) {
      if (closed || !keyValid(key)) return

      try {
        const record = await transact(database, 'readonly', (store, done) => {
          const request = store.get(slot)
          request.onsuccess = () => done(request.result)
        })

        if (record?.version !== 1 || record.key !== key || !Array.isArray(record.hashes)) return

        const parsed = payload(record.value)

        if (!parsed || record.hashes.length !== 3) return

        const actual = await hashes(parsed)

        if (actual.some((hash, index) => hash !== record.hashes[index])) return

        return parsed.value
      } catch {
        return undefined
      }
    },
    async put(key, value) {
      if (closed || !keyValid(key)) return false

      try {
        const parsed = payload(value)

        if (!parsed) return false

        const record = { version: 1, key, value: parsed.value, hashes: await hashes(parsed) }

        if (closed) return false

        return (
          (await transact(database, 'readwrite', (store, done) => {
            const request = store.put(record, slot)
            request.onsuccess = () => done(true)
          })) === true
        )
      } catch {
        return false
      }
    },
    async delete(key) {
      if (closed || !keyValid(key)) return false

      return (
        (await transact(database, 'readwrite', (store, done) => {
          const request = store.get(slot)

          request.onsuccess = () => {
            try {
              if (request.result?.key !== key) return done(false)

              const removal = store.delete(slot)
              removal.onsuccess = () => done(true)
            } catch {
              done(false)
            }
          }
        })) === true
      )
    },
    close,
  }
}
