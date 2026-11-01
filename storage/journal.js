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
          storage.close()
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
  const entry = await directory.getFileHandle(`${identity}.journal`, { create: true })
  const file = await entry.createSyncAccessHandle()
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
  } catch (error) {
    file.close()
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
