import { LauncherMusic } from '../audio/launcher-music.js'
import { FolderSelection } from './folder-selection.js'
import { createProgressPanel } from './progress.js'
import { checkForAppUpdate } from './pwa.js'

export class Launcher {
  constructor() {
    this.shell = document.querySelector('.launch-shell')
    this.form = document.querySelector('#game-form')
    this.playButton = this.form.querySelector('button[type=submit]')
    this.help = document.querySelector('#folder-help')
    this.busy = false

    this.selection = new FolderSelection({
      input: this.form.elements.folder,
      picker: document.querySelector('.folder-picker'),
      help: this.help,
      isAvailable: () => this.visible && !this.busy,
      onChange: this.updateControls,
    })

    this.music = new LauncherMusic(document.querySelector('#launcher-music'), () => this.visible)
    this.bindDialogs()
    this.bindProgress()
    this.updateControls()
  }

  get visible() {
    return !this.shell.hidden
  }

  get available() {
    return this.visible && !this.busy && !this.selection.choosing
  }

  bindDialogs() {
    const dialogs = [
      ['#about', '#information'],
      ['#maxis', '#maxis-easter-egg'],
    ]

    for (const [button, dialog] of dialogs) {
      document.querySelector(button).addEventListener('click', () => {
        document.querySelector(dialog).showModal()
      })
    }
  }

  bindProgress() {
    createProgressPanel({
      isAvailable: () => this.available,
      onBusy: this.setBusy,
      onMessage: (message) => {
        this.help.textContent = message
      },
    })
  }

  onPlay({ prepare, start }) {
    this.prepare = prepare
    this.start = start
    this.form.addEventListener('submit', this.play)
  }

  updateControls = () => {
    this.playButton.disabled = !this.available || !this.selection.selected
    this.selection.input.disabled = this.busy || this.selection.choosing
  }

  setBusy = (busy) => {
    this.busy = busy
    this.updateControls()
  }

  play = async (event) => {
    event.preventDefault()

    if (!this.available || this.playButton.disabled) return

    this.setBusy(true)
    let files

    try {
      // Audio must resume within the Play gesture, before the first await.
      this.prepare()

      if (!(await checkForAppUpdate())) return

      files = await this.selection.read()
    } catch (error) {
      this.help.textContent = `Could not start game. ${error.message}`

      return
    } finally {
      this.setBusy(false)
    }

    this.hide()
    this.start(files)
  }

  hide() {
    this.shell.hidden = true
    this.help.hidden = true
    this.music.stop()
    this.updateControls()
  }

  show = () => {
    this.shell.hidden = false
    this.help.hidden = false
    this.updateControls()
    void this.music.resume()
  }
}
