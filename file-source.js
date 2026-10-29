// Selected files stay in browser-managed storage. A shared LRU holds at most
// 8 MiB of immutable 64 KiB pages, independent of the total installation size.
export function createFileReaders(
  readBlob,
  budget = 8 * 1024 * 1024,
  pageSize = 64 * 1024,
  { readPage, stats = {} } = {},
) {
  if (
    !Number.isSafeInteger(budget) ||
    budget < 0 ||
    !Number.isSafeInteger(pageSize) ||
    pageSize < 1
  ) {
    throw new RangeError('Invalid game file cache size')
  }

  Object.assign(stats, { memoryHits: 0, memoryMisses: 0, sourceBytes: 0, sourceReadMs: 0 })
  const cache = new Map()
  let resident = 0
  let nextId = 0

  return (file, path = '') => {
    const id = nextId++

    return (offset, length) => {
      if (
        !Number.isSafeInteger(offset) ||
        !Number.isSafeInteger(length) ||
        offset < 0 ||
        length < 0 ||
        offset + length > file.size
      ) {
        throw new RangeError('Invalid game file read')
      }

      const result = new Uint8Array(length)
      let written = 0

      while (written < length) {
        const position = offset + written
        const start = Math.floor(position / pageSize) * pageSize
        const key = `${id}:${start}`
        let page = cache.get(key)

        if (page) {
          stats.memoryHits++
          cache.delete(key)
        } else {
          stats.memoryMisses++

          const original = (offset, count) => {
            const began = performance.now()
            const bytes = new Uint8Array(readBlob(file.slice(offset, offset + count)))
            stats.sourceReadMs += performance.now() - began
            stats.sourceBytes += bytes.byteLength

            return bytes
          }

          const count = Math.min(pageSize, file.size - start)
          page = readPage ? readPage(file, path, start, count, original) : original(start, count)

          if (page.length !== Math.min(pageSize, file.size - start)) {
            throw new Error('Short read from selected game file')
          }

          resident += page.byteLength
        }

        cache.set(key, page)

        while (resident > budget && cache.size) {
          const oldest = cache.keys().next().value
          resident -= cache.get(oldest).byteLength
          cache.delete(oldest)
        }

        const count = Math.min(length - written, page.length - (position - start))
        result.set(page.subarray(position - start, position - start + count), written)
        written += count
      }

      return result
    }
  }
}
