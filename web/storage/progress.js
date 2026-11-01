import { hashBytes } from '../shared/hash.js'
import { requireVerifiedBackup, createProgressBackup } from './backup-format.js'
import {
  acquireSaveLock,
  savesDirectory,
  registryKey,
  generationKey,
  journalName,
  registryPrefix,
  registryEntries,
  saveDirectory,
} from './save-paths.js'

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

    localStorage.removeItem(generationKey)
  } finally {
    await release()
  }
}

export async function exportProgress() {
  const release = await acquireSaveLock('exclusive')

  try {
    const root = await navigator.storage.getDirectory()
    let directory

    try {
      directory = await saveDirectory(root)
    } catch (error) {
      if (error.name !== 'NotFoundError') throw error
    }

    const files = []
    const journals = []

    if (directory) {
      for await (const [name, handle] of directory.entries()) {
        if (handle.kind !== 'file') continue

        if (!journalName.test(name)) throw new Error('An unexpected file is in saved progress.')

        // Copy while locked: an OPFS File snapshot can be invalidated by later game writes.
        const bytes = await (await handle.getFile()).arrayBuffer()
        journals.push({ name, size: bytes.byteLength, sha256: await hashBytes(bytes) })
        files.push(new Blob([bytes]))
      }
    }

    const registry = registryEntries(registryPrefix())

    if (!journals.length && !registry.length)
      throw new Error('There is no saved progress to export yet.')

    return createProgressBackup(files, journals, registry)
  } finally {
    await release()
  }
}

export async function importProgress(backup) {
  requireVerifiedBackup(backup)

  const generation = crypto.randomUUID().replaceAll('-', '')
  const prefix = registryPrefix(generation)
  const release = await acquireSaveLock('exclusive')
  let directory
  let committed = false

  try {
    const root = await navigator.storage.getDirectory()
    directory = await root.getDirectoryHandle(savesDirectory, { create: true })
    const staging = await directory.getDirectoryHandle(generation, { create: true })

    for (const { name, data } of backup.files) {
      const entry = await staging.getFileHandle(name, { create: true })
      const writer = await entry.createWritable()

      try {
        await writer.write(data)
        await writer.close()
      } catch (error) {
        await writer.abort().catch(() => {})
        throw error
      }
    }

    for (const { clientId, snapshot } of backup.registry)
      localStorage.setItem(`${prefix}${clientId}`, snapshot)

    // One atomic pointer switches both saves and preferences after all writes succeed.
    localStorage.setItem(generationKey, generation)
    committed = true

    // Old generations and interrupted imports are no longer referenced. Cleanup
    // failure must not turn a committed, usable restore into an apparent failure.
    try {
      for await (const [name] of directory.entries()) {
        if (name !== generation && (journalName.test(name) || /^[0-9a-f]{32}$/.test(name)))
          await directory.removeEntry(name, { recursive: true })
      }

      for (let index = localStorage.length - 1; index >= 0; index--) {
        const key = localStorage.key(index)

        if (key?.startsWith(`${registryKey}:`) && !key.startsWith(prefix))
          localStorage.removeItem(key)
      }
    } catch {
      /* Unreferenced data can be cleaned up by the next import or reset. */
    }
  } finally {
    try {
      if (!committed) {
        await directory?.removeEntry(generation, { recursive: true }).catch(() => {})

        for (const { clientId } of backup.registry) {
          try {
            localStorage.removeItem(`${prefix}${clientId}`)
          } catch {
            /* Uncommitted preferences cannot be loaded without the pointer. */
          }
        }
      }
    } finally {
      await release()
    }
  }
}
