import {
  exportProgress,
  importProgress,
  inspectProgressBackup,
  resetProgress,
} from '../storage/index.js'

export function createProgressPanel({ isAvailable, onBusy, onMessage }) {
  return new ProgressPanel({ isAvailable, onBusy, onMessage })
}

class ProgressPanel {
  constructor({ isAvailable, onBusy, onMessage }) {
    Object.assign(this, { isAvailable, onBusy, onMessage })
    this.panel = document.querySelector('#progress-dialog')
    this.status = document.querySelector('#progress-status')
    this.input = document.querySelector('#progress-file')
    this.resetDialog = document.querySelector('#reset-progress-dialog')
    this.resetStatus = document.querySelector('#reset-status')
    this.importDialog = document.querySelector('#import-progress-dialog')
    this.importStatus = document.querySelector('#import-status')
    this.dialogs = [this.panel, this.resetDialog, this.importDialog]
    this.controls = this.dialogs.flatMap((dialog) => [...dialog.querySelectorAll('button')])
    this.busy = false
    this.pendingBackup = undefined
    this.bindEvents()
  }

  bindEvents() {
    for (const dialog of this.dialogs) {
      dialog.addEventListener('cancel', this.onCancel)
    }

    document.querySelector('#reset-progress').addEventListener('click', this.open)
    document.querySelector('#export-progress').addEventListener('click', this.exportBackup)
    document.querySelector('#import-progress').addEventListener('click', this.chooseImport)
    this.input.addEventListener('change', this.inspectBackup)
    document.querySelector('#cancel-import').addEventListener('click', this.cancelImport)
    this.importDialog.addEventListener('close', this.clearImport)
    document.querySelector('#confirm-import').addEventListener('click', this.restoreBackup)
    document.querySelector('#show-reset-progress').addEventListener('click', this.showReset)
    document.querySelector('#cancel-reset').addEventListener('click', this.cancelReset)
    document.querySelector('#confirm-reset').addEventListener('click', this.resetProgress)
  }

  setBusy = (value) => {
    this.busy = value
    this.controls.forEach((button) => {
      button.disabled = value
    })
    this.input.disabled = value
    this.onBusy(value)
  }

  success = (message) => {
    this.status.textContent = message
    this.onMessage(message)
  }

  onCancel = (event) => {
    if (this.busy) event.preventDefault()
  }

  open = () => {
    if (this.busy || !this.isAvailable()) return

    this.status.textContent = ''
    this.panel.showModal()
  }

  exportBackup = async () => {
    if (this.busy || !this.isAvailable()) return

    this.setBusy(true)
    this.status.textContent = 'Preparing your progress backup…'

    try {
      const { blob, createdAt } = await exportProgress()
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = `the-sims-progress-${createdAt.slice(0, 10)}.sims-save`
      link.hidden = true
      document.body.append(link)
      link.click()
      link.remove()
      setTimeout(() => URL.revokeObjectURL(url), 60_000)
      this.status.textContent = 'Backup exported. Keep it somewhere safe.'
    } catch (error) {
      this.status.textContent = `Could not export progress. ${error.message}`
    } finally {
      this.setBusy(false)
    }
  }

  chooseImport = () => {
    if (this.busy || !this.isAvailable()) return

    this.input.value = ''
    this.input.click()
  }

  inspectBackup = async () => {
    const file = this.input.files[0]

    if (!file || this.busy || !this.isAvailable()) return

    this.setBusy(true)
    this.pendingBackup = undefined
    this.status.textContent = 'Checking your progress backup…'

    try {
      this.pendingBackup = await inspectProgressBackup(file)
      document.querySelector('#import-summary').textContent =
        `Backup created ${new Date(this.pendingBackup.createdAt).toLocaleString()}.`
      this.importStatus.textContent = ''
      this.status.textContent = ''
    } catch (error) {
      this.status.textContent = `Could not read this backup. ${error.message}`
    } finally {
      this.input.value = ''
      this.setBusy(false)
    }

    if (this.pendingBackup) this.importDialog.showModal()
  }

  cancelImport = () => this.importDialog.close()

  clearImport = () => {
    this.pendingBackup = undefined
  }

  restoreBackup = async () => {
    if (this.busy || !this.pendingBackup || !this.isAvailable()) return

    this.setBusy(true)
    this.importStatus.textContent = 'Restoring saved progress…'

    try {
      await importProgress(this.pendingBackup)
      this.importDialog.close()
      this.success('Progress imported. Choose the same original game folder, then press Play.')
    } catch (error) {
      this.importStatus.textContent = `Could not import progress. ${error.message}`
    } finally {
      this.setBusy(false)
    }
  }

  showReset = () => {
    if (this.busy || !this.isAvailable()) return

    this.resetStatus.textContent = ''
    this.resetDialog.showModal()
  }

  cancelReset = () => this.resetDialog.close()

  resetProgress = async () => {
    if (this.busy || !this.isAvailable()) return

    this.setBusy(true)
    this.resetStatus.textContent = 'Deleting saved progress…'

    try {
      await resetProgress()
      this.resetDialog.close()
      this.success('Progress reset. Your next game starts from your original game folder.')
    } catch (error) {
      this.resetStatus.textContent = `Could not finish resetting progress. ${error.message}`
    } finally {
      this.setBusy(false)
    }
  }
}
