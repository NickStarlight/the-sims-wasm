export const registryKey = 'sims-web:registry-v2'
export const savesDirectory = 'sims-web-saves-v1'
const saveLock = 'sims-web:save-storage'
export const generationKey = 'sims-web:progress-generation-v1'
export const journalName = /^[0-9a-f]{64}-[0-9a-f]{64}\.journal$/

export function currentGeneration() {
  let generation

  try {
    generation = localStorage.getItem(generationKey)
  } catch {
    return null
  }

  if (generation && !/^[0-9a-f]{32}$/.test(generation))
    throw new Error('Saved progress metadata is invalid.')

  return generation || null
}

export function registryPrefix(generation = currentGeneration()) {
  return generation ? `${registryKey}:${generation}:` : `${registryKey}:`
}

export async function saveDirectory(root, create = false) {
  const directory = await root.getDirectoryHandle(savesDirectory, { create })
  const generation = currentGeneration()

  return generation ? directory.getDirectoryHandle(generation, { create }) : directory
}

export function registryEntries(prefix) {
  const entries = []

  for (let index = 0; index < localStorage.length; index++) {
    const key = localStorage.key(index)

    if (!key?.startsWith(prefix)) continue

    const clientId = key.slice(prefix.length)

    if (!/^[0-9a-f]{64}$/.test(clientId)) continue

    entries.push({ clientId, snapshot: localStorage.getItem(key) })
  }

  return entries.sort((a, b) => a.clientId.localeCompare(b.clientId))
}

// A running game holds a shared lease; progress management requires exclusive access.
export function acquireSaveLock(mode) {
  return new Promise((resolve, reject) => {
    const request = navigator.locks.request(saveLock, { mode, ifAvailable: true }, (lock) => {
      if (!lock)
        throw new Error(
          'Close other running game tabs before managing progress or starting a game.',
        )

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
