import { ErrorReport } from './error-report.js'

export class GameView {
  constructor() {
    this.panel = document.querySelector('.game-panel')
    this.status = document.querySelector('#play-status')
    this.errors = new ErrorReport()
  }

  start() {
    this.panel.hidden = false
    this.errors.clear()
    this.status.textContent = 'Loading game…'
    this.status.classList.add('compiling')
  }

  ready() {
    this.status.textContent = ''
    this.status.classList.remove('compiling')
  }

  progress(message) {
    this.status.textContent = message
  }

  finish(message, stack) {
    this.panel.hidden = true
    this.status.classList.remove('compiling')
    this.status.textContent = message
    this.errors.show(message, stack)
  }
}
