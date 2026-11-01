import { validateClientId } from '../compiler/identity.js'
import { hashBytes } from '../shared/hash.js'
import { acquireSaveLock, saveDirectory } from './save-paths.js'

export async function openSaveStorage(files, process, clientId) {
  validateClientId(clientId)

  const release = await acquireSaveLock('shared')

  try {
    const storage = await openJournal(files, process, clientId)

    return {
      flush: () => storage.flush(),
      async close() {
        try {
          await storage.close()
        } finally {
          await release()
        }
      },
    }
  } catch (error) {
    await release()
    throw error
  }
}

async function openJournal(files, process, clientId) {
  const find = (path) =>
    files.find((entry) => entry.path.replaceAll('\\', '/').toLowerCase() === path)?.file

  const hash = async (file) => {
    const bytes = file ? await file.arrayBuffer() : new ArrayBuffer(0)

    return hashBytes(bytes)
  }

  const identity = `${clientId}-${await hash(find('userdata/neighborhood.iff'))}`
  const root = await navigator.storage.getDirectory()
  const directory = await saveDirectory(root, true)
  const releaseJournal = await acquireSaveLock('exclusive', `sims-web:journal:${identity}`)

  try {
    const storage = await openJournalFile(directory, process, identity)

    return {
      flush: storage.flush,
      async close() {
        try {
          await storage.close()
        } finally {
          await releaseJournal()
        }
      },
    }
  } catch (error) {
    await releaseJournal()
    throw error
  }
}

async function openJournalFile(directory, process, identity) {
  const entry = await directory.getFileHandle(`${identity}.journal`, { create: true })
  let file = await entry.createSyncAccessHandle()
  let end
  let failed = false

  try {
    const bytes = new Uint8Array(file.getSize())
    transfer(file, 'read', bytes, 0)
    end = process.restoreJournal(bytes)

    if (end !== bytes.length) {
      file.truncate(end)
      file.flush()
    }

    // Rewrite only substantial histories with at least 50% reclaimable space.
    // createWritable commits its temporary file on close; never truncate the
    // original journal to install a snapshot. The per-journal lock spans the
    // gap between releasing and reacquiring the synchronous handle.
    if (end >= 16 * 1024 * 1024 && typeof entry.createWritable === 'function') {
      const snapshot = process.compactJournal(bytes.subarray(0, end))

      if (snapshot.length <= end / 2) {
        file.close()
        file = undefined
        const writer = await entry.createWritable()

        try {
          await writer.write(snapshot)
          await writer.close()
        } catch (error) {
          await writer.abort().catch(() => {})
          throw error
        }

        file = await entry.createSyncAccessHandle()
        end = snapshot.length
      }
    }
  } catch (error) {
    file?.close()
    throw error
  }

  return {
    flush() {
      if (failed) throw new Error('Save storage failed; reload before continuing.')

      const bytes = process.takeFileChanges()

      if (!bytes.length) return

      try {
        transfer(file, 'write', bytes, end)
        file.flush()
        end += bytes.length
      } catch (error) {
        failed = true

        try {
          file.truncate(end)
          file.flush()
        } catch {
          /* Preserve for recovery. */
        }

        throw new Error(`Could not persist game save: ${error.message}`)
      }
    },
    close() {
      file.close()
    },
  }
}

function transfer(file, operation, bytes, offset) {
  let count = 0

  while (count < bytes.length) {
    const size = file[operation](bytes.subarray(count), { at: offset + count })

    if (size <= 0 || size > bytes.length - count)
      throw new Error(`Save storage ${operation} failed.`)

    count += size
  }
}
