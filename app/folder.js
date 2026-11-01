// Keep the directory capability, rather than an unusable local filesystem path.
export async function savedFolder(value) {
  const database = await new Promise((resolve, reject) => {
    const request = indexedDB.open('sims-web-folder', 1)
    request.onupgradeneeded = () => request.result.createObjectStore('settings')
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })

  try {
    return await new Promise((resolve, reject) => {
      const transaction = database.transaction('settings', value ? 'readwrite' : 'readonly')
      const settings = transaction.objectStore('settings')
      const request = value ? settings.put(value, 'folder') : settings.get('folder')
      transaction.oncomplete = () => resolve(value ?? request.result)
      transaction.onabort = () => reject(transaction.error)
      transaction.onerror = () => reject(transaction.error)
    })
  } finally {
    database.close()
  }
}

export async function folderFiles(directory, prefix = '') {
  const files = []

  for await (const entry of directory.values()) {
    const path = `${prefix}${entry.name}`

    if (entry.kind === 'directory') files.push(...(await folderFiles(entry, `${path}/`)))
    else files.push({ file: await entry.getFile(), path })
  }

  return files
}
