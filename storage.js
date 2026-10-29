// Browser storage only. Rust owns registry serialization and the existing SMS1 save format.
import { validateClientId } from './game.js'

const registryKey = 'sims-web:registry-v2'
const savesDirectory = 'sims-web-saves-v1'
const saveLock = 'sims-web:save-storage'

export function loadRegistry(clientId) {
  validateClientId(clientId)

  try {
    return localStorage.getItem(`${registryKey}:${clientId}`)
  } catch {
    return null
  }
}

export function storeRegistry(snapshot, clientId) {
  validateClientId(clientId)

  try {
    localStorage.setItem(`${registryKey}:${clientId}`, snapshot)
  } catch {
    /* Registry remains usable when storage is unavailable. */
  }
}

// A running game holds a shared lease; reset requires exclusive access across tabs.
function acquireSaveLock(mode) {
  return new Promise((resolve, reject) => {
    const request = navigator.locks.request(saveLock, { mode, ifAvailable: true }, (lock) => {
      if (!lock)
        throw new Error('Close other running game tabs before resetting or starting a game.')

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

export async function resetProgress() {
  const release = await acquireSaveLock('exclusive')

  try {
    const root = await navigator.storage.getDirectory()

    try {
      await root.removeEntry(savesDirectory, { recursive: true })
    } catch (error) {
      if (error.name !== 'NotFoundError') throw error
    }

    for (let index = localStorage.length - 1; index >= 0; index--) {
      const key = localStorage.key(index)

      if (key?.startsWith(`${registryKey}:`)) localStorage.removeItem(key)
    }
  } finally {
    await release()
  }
}

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
    const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))

    return [...digest].map((byte) => byte.toString(16).padStart(2, '0')).join('')
  }

  const identity = `${clientId}-${await hash(find('userdata/neighborhood.iff'))}`
  const root = await navigator.storage.getDirectory()
  const directory = await root.getDirectoryHandle(savesDirectory, { create: true })
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
