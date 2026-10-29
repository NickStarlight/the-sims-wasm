// Optional immutable source pages, separate from native save journals. Only two
// OPFS handles stay open. A lost/torn cache always falls back to the selected file.
const pageSize = 64 * 1024
const maximumBudget = 512 * 1024 * 1024
const cacheDirectory = 'sims-web-assets-v1'
const lockName = 'sims-web:asset-cache'
const magic = 0x31434153 // SAC1
const extensions = new Set(
  'far iff bmp jpg jpeg png gif tga pcx dds wll flr skn cmx bcf bmf pal 256 fnt ttf otf cur ico wav mp3 xa utk hit hot hsm lng avi'.split(
    ' ',
  ),
)

function assetPath(path) {
  const parts = path
    .replaceAll('\\', '/')
    .toLowerCase()
    .split('/')
    .filter((part) => part !== '.')

  if (
    parts.some(
      (part) =>
        !part || part === '..' || part.includes(':') || /^(userdata|temp|save|cache)/.test(part),
    )
  )
    return null

  const name = parts.at(-1)

  if (!name || !extensions.has(name.split('.').at(-1))) return null

  return parts.join('/')
}

const crcTable = new Uint32Array(8 * 256)

for (let byte = 0; byte < 256; byte++) {
  let value = byte

  for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0)

  crcTable[byte] = value >>> 0
}

for (let i = 256; i < crcTable.length; i++) {
  const previous = crcTable[i - 256]
  crcTable[i] = (previous >>> 8) ^ crcTable[previous & 255]
}

// IEEE CRC32, preserving the persisted page/index wire format. Process eight
// bytes at once while retaining a byte tail for short and unaligned views.
export function checksum(bytes) {
  let value = 0xffffffff
  let i = 0

  for (; i + 8 <= bytes.length; i += 8) {
    value ^= bytes[i] | (bytes[i + 1] << 8) | (bytes[i + 2] << 16) | (bytes[i + 3] << 24)
    value =
      crcTable[0x700 | (value & 255)] ^
      crcTable[0x600 | ((value >>> 8) & 255)] ^
      crcTable[0x500 | ((value >>> 16) & 255)] ^
      crcTable[0x400 | (value >>> 24)] ^
      crcTable[0x300 | bytes[i + 4]] ^
      crcTable[0x200 | bytes[i + 5]] ^
      crcTable[0x100 | bytes[i + 6]] ^
      crcTable[bytes[i + 7]]
  }

  for (; i < bytes.length; i++) value = (value >>> 8) ^ crcTable[(value ^ bytes[i]) & 255]

  return (value ^ 0xffffffff) >>> 0
}

function transfer(handle, operation, bytes, offset) {
  let done = 0

  while (done < bytes.length) {
    const count = handle[operation](bytes.subarray(done), { at: offset + done })

    if (!Number.isInteger(count) || count <= 0 || count > bytes.length - done)
      throw new Error('Incomplete asset cache transfer')

    done += count
  }
}

async function acquire() {
  return new Promise((resolve, reject) => {
    const request = navigator.locks.request(lockName, { ifAvailable: true }, (lock) => {
      if (!lock) return resolve(null)

      return new Promise((release) =>
        resolve(() => {
          release()

          return request
        }),
      )
    })
    request.catch(reject)
  })
}

// readOriginal(offset, length) is synchronous and returns the selected file's bytes.
// Metadata invalidation catches normal edits, additions/removals and renames. An
// edit preserving both byte length and lastModified requires clearing this cache.
export async function openAssetCache(files, clientId, { budget = maximumBudget } = {}) {
  const stats = {
    status: 'unavailable',
    hits: 0,
    misses: 0,
    sourceBytes: 0,
    cacheBytes: 0,
    writes: 0,
    evictions: 0,
    corruptions: 0,
    pages: 0,
  }
  const pages = new Map()
  const assets = new Map()
  let manifest
  let capacity
  let data
  let index
  let release
  let released
  let dirty = false
  let closed = false
  let free = []

  const stop = (status) => {
    stats.status = status

    for (const handle of [data, index]) {
      try {
        handle?.close()
      } catch {
        /* Optional storage cannot stop the game. */
      }
    }

    data = index = undefined

    if (release) {
      released = release().catch(() => {})
      release = undefined
    }
  }

  const source = (start, length, readOriginal) => {
    const value = readOriginal(start, length)
    const bytes = value instanceof Uint8Array ? value : new Uint8Array(value)

    if (bytes.length !== length) throw new Error('Short read from selected game file')

    stats.sourceBytes += bytes.length

    return bytes
  }

  const flush = () => {
    if (!data || !dirty) return

    try {
      // Publish the index only after all referenced pages have been flushed.
      data.flush()
      const body = new TextEncoder().encode(
        JSON.stringify({
          manifest,
          pageSize,
          capacity,
          entries: [...pages.values()].map(({ fileId, page, slot, crc }) => [
            fileId,
            page,
            slot,
            crc,
          ]),
        }),
      )
      const bytes = new Uint8Array(12 + body.length)
      const header = new DataView(bytes.buffer)
      header.setUint32(0, magic, true)
      header.setUint32(4, body.length, true)
      header.setUint32(8, checksum(body), true)
      bytes.set(body, 12)
      transfer(index, 'write', bytes, 0)
      index.truncate(bytes.length)
      index.flush()
      dirty = false
    } catch {
      stop('failed')
    }
  }

  const cache = {
    stats,
    read(file, path, start, length, readOriginal) {
      if (
        !Number.isSafeInteger(start) ||
        !Number.isSafeInteger(length) ||
        start < 0 ||
        length < 0 ||
        start + length > file.size
      )
        throw new RangeError('Invalid game file read')

      const asset = assets.get(assetPath(path))

      if (!data || !asset || asset.file !== file) return source(start, length, readOriginal)

      const result = new Uint8Array(length)
      let done = 0

      while (done < length) {
        const position = start + done
        const page = Math.floor(position / pageSize)
        const offset = page * pageSize
        const count = Math.min(pageSize, file.size - offset)
        const key = `${asset.id}:${page}`
        const entry = pages.get(key)
        let bytes

        if (data && entry) {
          try {
            bytes = new Uint8Array(count)
            transfer(data, 'read', bytes, entry.slot * pageSize)

            if (checksum(bytes) !== entry.crc) {
              stats.corruptions++
              free.push(entry.slot)
              pages.delete(key)
              dirty = true
              bytes = undefined
            } else {
              stats.hits++
              stats.cacheBytes += count
              pages.delete(key)
              pages.set(key, entry)
              dirty = true // Retain LRU order across launches.
            }
          } catch {
            bytes = undefined
            stop('failed')
          }
        }

        if (!bytes) {
          stats.misses++
          // Source errors remain real errors; only cache failures are optional.
          bytes = source(offset, count, readOriginal)

          if (data) {
            try {
              let slot = free.pop()

              if (slot === undefined) {
                const oldest = pages.keys().next().value
                slot = pages.get(oldest).slot
                pages.delete(oldest)
                stats.evictions++
              }

              transfer(data, 'write', bytes, slot * pageSize)
              pages.set(key, { fileId: asset.id, page, slot, crc: checksum(bytes) })
              stats.writes++
              dirty = true
            } catch {
              stop('failed')
            }
          }
        }

        stats.pages = pages.size
        const copied = Math.min(length - done, count - (position - offset))
        result.set(bytes.subarray(position - offset, position - offset + copied), done)
        done += copied
      }

      return result
    },
    flush,
    async close() {
      if (!closed) {
        closed = true
        flush()
        stop('closed')
      }

      await released
    },
  }

  try {
    if (
      !clientId ||
      !globalThis.navigator?.storage?.getDirectory ||
      !navigator.locks?.request ||
      !Number.isSafeInteger(budget) ||
      budget < pageSize ||
      budget > maximumBudget
    )
      return cache

    capacity = Math.floor(budget / pageSize)
    const selected = files
      .map(({ file, path }) => ({ file, path: assetPath(path) }))
      .filter(
        ({ file, path }) =>
          path &&
          Number.isSafeInteger(file.size) &&
          file.size >= 0 &&
          Number.isSafeInteger(file.lastModified) &&
          file.lastModified >= 0,
      )
      .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))

    for (const [id, asset] of selected.entries()) {
      if (assets.has(asset.path)) return cache // Ambiguous case-insensitive source paths.

      assets.set(asset.path, { ...asset, id })
    }

    if (!selected.length) return cache

    const identity = JSON.stringify([
      1,
      clientId,
      selected.map(({ file, path }) => [path, file.size, file.lastModified]),
    ])
    manifest = [
      ...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(identity))),
    ]
      .map((byte) => byte.toString(16).padStart(2, '0'))
      .join('')
    release = await acquire()

    if (!release) {
      stats.status = 'busy'

      return cache
    }

    const root = await navigator.storage.getDirectory()
    const directory = await root.getDirectoryHandle(cacheDirectory, { create: true })
    data = await (
      await directory.getFileHandle('pages.bin', { create: true })
    ).createSyncAccessHandle()
    index = await (
      await directory.getFileHandle('index.bin', { create: true })
    ).createSyncAccessHandle()
    const used = new Set()

    try {
      const size = index.getSize()

      if (size < 12 || size > 1024 + capacity * 96 || data.getSize() > capacity * pageSize)
        throw new Error('Invalid asset cache size')

      const bytes = new Uint8Array(size)
      transfer(index, 'read', bytes, 0)
      const header = new DataView(bytes.buffer)
      const body = bytes.subarray(12)

      if (
        header.getUint32(0, true) !== magic ||
        header.getUint32(4, true) !== body.length ||
        header.getUint32(8, true) !== checksum(body)
      )
        throw new Error('Invalid asset cache index')

      const saved = JSON.parse(new TextDecoder().decode(body))

      if (
        saved.manifest !== manifest ||
        saved.pageSize !== pageSize ||
        !Number.isSafeInteger(saved.capacity) ||
        saved.capacity < 1 ||
        saved.capacity > capacity ||
        !Array.isArray(saved.entries) ||
        saved.entries.length > capacity
      )
        throw new Error('Changed asset cache identity')

      for (const record of saved.entries) {
        if (!Array.isArray(record) || record.length !== 4 || !record.every(Number.isSafeInteger))
          throw new Error('Invalid asset cache record')

        const [fileId, page, slot, crc] = record
        const file = selected[fileId]?.file
        const offset = page * pageSize
        const key = `${fileId}:${page}`

        if (
          !file ||
          page < 0 ||
          !Number.isSafeInteger(offset) ||
          offset >= file.size ||
          slot < 0 ||
          slot >= capacity ||
          used.has(slot) ||
          pages.has(key) ||
          crc < 0 ||
          crc > 0xffffffff ||
          slot * pageSize + Math.min(pageSize, file.size - offset) > data.getSize()
        )
          throw new Error('Invalid asset cache record')

        pages.set(key, { fileId, page, slot, crc })
        used.add(slot)
      }
    } catch {
      // Identity changes and corrupt metadata discard only the optional cache.
      pages.clear()
      used.clear()
      data.truncate(0)
      index.truncate(0)
      dirty = true
    }

    free = Array.from({ length: capacity }, (_, slot) => slot)
      .filter((slot) => !used.has(slot))
      .reverse()
    stats.pages = pages.size
    stats.status = 'ready'
  } catch {
    stop('unavailable')
  }

  return cache
}
