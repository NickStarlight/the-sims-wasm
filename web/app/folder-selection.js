import { folderFiles, savedFolder } from './folder.js'

export class FolderSelection {
  constructor({ input, picker, help, isAvailable, onChange }) {
    Object.assign(this, { input, help, isAvailable, onChange })
    this.directory = undefined
    this.files = []
    this.choosing = false

    input.addEventListener('change', this.useFiles)

    if ('showDirectoryPicker' in window) {
      input.required = false
      picker.addEventListener('click', this.chooseDirectory)
      void this.restoreDirectory()
    }
  }

  get selected() {
    return Boolean(this.directory || this.files.length)
  }

  describe(name) {
    this.help.textContent = name
      ? `${name} selected — ready to play.`
      : 'Choose your installed game folder, then press Play.'
    this.onChange()
  }

  async restoreDirectory() {
    try {
      const saved = await savedFolder()

      if (!saved || this.selected || this.choosing || !this.isAvailable()) return

      this.directory = saved
      this.describe(saved.name)
    } catch {
      // Choosing a folder remains available when IndexedDB is unavailable.
    }
  }

  chooseDirectory = async (event) => {
    event.preventDefault()

    if (this.choosing || !this.isAvailable()) return

    this.choosing = true
    this.onChange()

    try {
      this.directory = await window.showDirectoryPicker({
        id: 'sims-game',
        mode: 'read',
        startIn: this.directory ?? 'documents',
      })
      this.files = []
      await savedFolder(this.directory).catch(() => {})
      this.describe(this.directory.name)
    } catch (error) {
      if (error.name !== 'AbortError') {
        this.help.textContent = `Could not open folder. ${error.message}`
      }
    } finally {
      this.choosing = false
      this.onChange()
    }
  }

  useFiles = () => {
    this.directory = undefined
    this.files = [...this.input.files].map(selectedFile)
    this.describe(this.input.files[0]?.webkitRelativePath.split('/')[0])
  }

  async read() {
    let files = this.files

    if (this.directory) {
      const permission = await this.directory.requestPermission({ mode: 'read' })

      if (permission !== 'granted') {
        throw new Error('Allow access to your saved game folder to play.')
      }

      this.help.textContent = 'Reading game folder…'
      files = await folderFiles(this.directory)
    }

    if (!files.length) {
      throw new Error('The selected folder is empty. Choose your installed game folder.')
    }

    return files
  }
}

function selectedFile(file) {
  const path = file.webkitRelativePath
    ? file.webkitRelativePath.split('/').slice(1).join('/')
    : file.name

  return { file, path }
}
