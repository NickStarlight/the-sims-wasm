import { BrowserAudio } from '../audio/audio-output.js'
import { createDisplay } from '../display/presentation.js'
import { loadRegistry, storeRegistry } from '../storage/index.js'
import { GameView } from './game-view.js'
import { createPerformancePanel } from './performance.js'

const compilerWorker = new URL('../workers/worker.js', import.meta.url)
const runtimeWorker = new URL('../workers/runtime-worker.js', import.meta.url)

export class GameSession {
  constructor({ onFinish }) {
    this.onFinish = onFinish
    this.worker = undefined
    this.audio = undefined
    this.running = false
    this.inputSequence = 0
    this.view = new GameView()

    this.display = createDisplay(
      document.querySelector('canvas'),
      this.sendInput,
      document.querySelector('#frame-status'),
      (frame, paintMs) => this.diagnostics.paint(frame, paintMs),
    )
    this.diagnostics = createPerformancePanel(this.display, this.sendDiagnostics)

    if (new URL(location.href).searchParams.get('perf') === '1') this.diagnostics.open()
  }

  resumeAudio = () => {
    this.audio ??= new BrowserAudio()
    this.audio.resume()
  }

  start = (files) => {
    try {
      this.running = false
      this.audio?.stop()
      this.display.reset()
      this.diagnostics.reset()
      this.view.start()
      this.replaceWorker(compilerWorker)
      this.worker.postMessage({ kind: 'run', files })
    } catch (error) {
      this.fail(error.message, error.stack)
    }
  }

  replaceWorker(source) {
    this.worker?.terminate()
    const worker = new Worker(source, { type: 'module' })
    this.worker = worker

    worker.onmessage = ({ data }) => {
      if (this.worker !== worker) return

      try {
        this.handleMessage(data)
      } catch (error) {
        this.fail(error.message, error.stack)
      }
    }

    worker.onerror = (event) => {
      if (this.worker === worker) {
        this.fail(`Worker failed: ${event.message}`, event.error?.stack)
      }
    }
  }

  handleMessage(data) {
    if (data.phase) return this.diagnostics.phase(data.phase)

    if (data.performance) return this.diagnostics.runtime(data.performance)

    if (data.registry !== undefined) return storeRegistry(data.registry, data.clientId)

    if (data.frame) {
      this.diagnostics.receive()
      this.display.present(data.frame)

      return
    }

    if (data.program) return this.run(data.program)

    if (data.progress && !this.running) this.view.progress(data.progress)

    if (data.finished) return this.finish()

    if (data.error) this.fail(data.error, data.stack)
  }

  run(program) {
    this.view.ready()
    this.running = true
    this.replaceWorker(runtimeWorker)
    this.worker.postMessage(
      {
        ...program,
        registry: loadRegistry(program.clientId),
        frameAcknowledgements: true,
        packedFrames: this.display.packedFrames,
        performanceEnabled: this.diagnostics.enabled,
      },
      [program.wasm.buffer, program.imageBytes.buffer],
    )
    this.diagnostics.program(program.clientId)
    void this.connectAudio(this.worker)
  }

  async connectAudio(worker) {
    if (!this.audio) return

    try {
      const port = await this.audio.connect()

      if (this.worker === worker && this.running)
        worker.postMessage({ kind: 'audio', port }, [port])
      else port.close()
    } catch (error) {
      console.warn('Audio output unavailable:', error.message)
    }
  }

  sendInput = (message) => {
    if (!this.running) return false

    if (this.diagnostics.enabled && (message.kind === 'mouse' || message.kind === 'key')) {
      message = {
        ...message,
        diagnostics: {
          sequence: ++this.inputSequence,
          sentAtMs: performance.timeOrigin + performance.now(),
        },
      }
    }

    this.worker.postMessage(message)

    return true
  }

  sendDiagnostics = (message) => {
    if (this.running) this.worker.postMessage(message)
  }

  fail(message, stack) {
    console.error(stack ?? message)
    this.finish(message, stack)
  }

  finish(message = '', stack = '') {
    this.worker?.terminate()
    this.worker = undefined
    this.audio?.stop()
    this.running = false
    this.view.finish(message, stack)
    this.onFinish()
  }
}
