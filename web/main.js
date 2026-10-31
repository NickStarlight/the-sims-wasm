import { savedFolder, folderFiles } from './folder.js'
import { createDisplay } from './presentation.js'
import { BrowserAudio } from './audio-output.js'
import { loadRegistry, storeRegistry, resetProgress } from './storage.js'
import { checkForAppUpdate } from './pwa.js'
import { createPerformancePanel } from './performance.js'

let audio

let worker = startWorker()
let executing = false
let resetting = false
let inputSequence = 0
const canvas = document.querySelector('canvas')
const display = createDisplay(
  canvas,
  (message) => {
    if (!executing) return false

    if (diagnostics?.enabled && (message.kind === 'mouse' || message.kind === 'key')) {
      message = {
        ...message,
        diagnostics: {
          sequence: ++inputSequence,
          sentAtMs: performance.timeOrigin + performance.now(),
        },
      }
    }

    worker.postMessage(message)

    return true
  },
  document.querySelector('#frame-status'),
  (frame, paintMs) => diagnostics?.paint(frame, paintMs),
)
const diagnostics = createPerformancePanel(display, (message) => {
  if (executing) worker.postMessage(message)
})

if (new URL(location.href).searchParams.get('perf') === '1') diagnostics.open()

const gameForm = document.querySelector('#game-form')
const playButton = gameForm.querySelector('button[type=submit]')
const playStatus = document.querySelector('#play-status')
const errorDetails = document.querySelector('#error-details')
const errorReport = document.querySelector('#error-report')
const copyErrorStatus = document.querySelector('#copy-error-status')
const launcher = document.querySelector('.launch-shell')
const gamePanel = document.querySelector('.game-panel')
const folderHelp = document.querySelector('#folder-help')
const launcherMusic = document.querySelector('#launcher-music')
let directory
let selectedFiles = []
let choosingFolder = false
let musicFinished = false

document.querySelector('#copy-error').addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(errorReport.value)
    copyErrorStatus.textContent = 'Copied. Paste these details with your bug report.'
  } catch {
    errorReport.hidden = false
    errorReport.focus()
    errorReport.select()
    copyErrorStatus.textContent = 'Copy the selected details below.'
  }
})

launcherMusic.volume = 0.45
const musicReady = fetch(launcherMusic.getAttribute('src'))
  .then((response) => {
    if (!response.ok) throw new Error('Launcher music could not be loaded.')

    return response.blob()
  })
  .then((blob) => {
    launcherMusic.src = URL.createObjectURL(blob)
  })
launcherMusic.addEventListener('ended', () => {
  musicFinished = true
})

async function resumeLauncherMusic() {
  try {
    await musicReady

    if (musicFinished || launcher.hidden) return

    await launcherMusic.play()

    if (musicFinished || launcher.hidden) launcherMusic.pause()
  } catch (error) {
    // Autoplay may need the first user gesture. Never restart a finished clip.
    if (error.name !== 'NotAllowedError' && error.name !== 'AbortError') musicFinished = true
  }
}

void resumeLauncherMusic()
document.addEventListener('click', () => {
  if (!musicFinished && !launcher.hidden && launcherMusic.paused) void resumeLauncherMusic()
})

function updateFolderSelection(name) {
  playButton.disabled = resetting || choosingFolder || (!directory && !selectedFiles.length)
  folderHelp.textContent = name
    ? `${name} selected — ready to play.`
    : 'Choose your installed game folder, then press Play.'
}

if ('showDirectoryPicker' in window) {
  gameForm.elements.folder.required = false
  document.querySelector('.folder-picker').addEventListener('click', async (event) => {
    event.preventDefault()

    if (choosingFolder || resetting) return

    choosingFolder = true
    playButton.disabled = true

    try {
      directory = await window.showDirectoryPicker({
        id: 'sims-game',
        mode: 'read',
        startIn: directory ?? 'documents',
      })
      selectedFiles = []
      await savedFolder(directory).catch(() => {})
    } catch (error) {
      if (error.name !== 'AbortError')
        folderHelp.textContent = `Could not open folder. ${error.message}`
    } finally {
      choosingFolder = false
      updateFolderSelection(directory?.name)
    }
  })
  savedFolder()
    .then((saved) => {
      if (!saved || directory || choosingFolder || selectedFiles.length) return

      directory = saved
      updateFolderSelection(directory.name)
    })
    .catch(() => {})
}

gameForm.elements.folder.addEventListener('change', () => {
  directory = undefined
  selectedFiles = [...gameForm.elements.folder.files].map((file) => ({
    file,
    path: file.webkitRelativePath
      ? file.webkitRelativePath.split('/').slice(1).join('/')
      : file.name,
  }))
  updateFolderSelection(gameForm.elements.folder.files[0]?.webkitRelativePath.split('/')[0])
})

document
  .querySelector('#about')
  .addEventListener('click', () => document.querySelector('#information').showModal())
const resetDialog = document.querySelector('#reset-progress-dialog')
const resetConfirm = document.querySelector('#confirm-reset')
const resetCancel = document.querySelector('#cancel-reset')
const resetStatus = document.querySelector('#reset-status')
document.querySelector('#reset-progress').addEventListener('click', () => {
  if (executing || launcher.hidden || resetting) return

  resetStatus.textContent = ''
  resetDialog.showModal()
})
resetCancel.addEventListener('click', () => resetDialog.close())
resetDialog.addEventListener('cancel', (event) => {
  if (resetting) event.preventDefault()
})
resetConfirm.addEventListener('click', async () => {
  if (resetting || executing || launcher.hidden) return

  resetting = true
  resetConfirm.disabled = true
  resetCancel.disabled = true
  playButton.disabled = true
  resetStatus.textContent = 'Deleting saved progress…'

  try {
    await resetProgress()
    resetDialog.close()
    folderHelp.textContent = 'Progress reset. Your next game starts from your original game folder.'
  } catch (error) {
    resetStatus.textContent = `Could not finish resetting progress. ${error.message}`
  } finally {
    resetting = false
    resetConfirm.disabled = false
    resetCancel.disabled = false
    playButton.disabled = !directory && !selectedFiles.length
  }
})

gameForm.addEventListener('submit', async (event) => {
  event.preventDefault()

  if (resetting || playButton.disabled) return

  playButton.disabled = true
  let files = selectedFiles

  try {
    // Resume during the Play gesture, before waiting for update or folder I/O.
    audio ??= new BrowserAudio()
    audio.resume()

    if (!(await checkForAppUpdate())) {
      playButton.disabled = false

      return
    }

    if (directory) {
      const permission = await directory.requestPermission({ mode: 'read' })

      if (permission !== 'granted')
        throw new Error('Allow access to your saved game folder to play.')

      folderHelp.textContent = 'Reading game folder…'
      files = await folderFiles(directory)
    }

    if (!files.length)
      throw new Error('The selected folder is empty. Choose your installed game folder.')
  } catch (error) {
    folderHelp.textContent = `Could not start game. ${error.message}`
    playButton.disabled = false

    return
  }

  musicFinished = true
  launcherMusic.pause()
  launcher.hidden = true
  folderHelp.hidden = true
  gamePanel.hidden = false
  errorDetails.hidden = true
  errorReport.value = ''
  audio.stop()
  executing = false
  display.reset()
  diagnostics.reset()
  worker.terminate()
  worker = startWorker()
  playButton.disabled = true
  playStatus.textContent = 'Loading game…'
  playStatus.classList.add('compiling')
  worker.postMessage({ kind: 'run', files })
})

function showResult({ data }) {
  if (data.phase) {
    diagnostics.phase(data.phase)

    return
  }

  if (data.performance) {
    diagnostics.runtime(data.performance)

    return
  }

  if (data.registry !== undefined) {
    storeRegistry(data.registry, data.clientId)

    return
  }

  if (data.frame) {
    diagnostics.receive()
    display.present(data.frame)

    return
  }

  if (data.program) {
    playStatus.textContent = ''
    playStatus.classList.remove('compiling')
    executing = true
    worker.terminate()
    worker = startWorker('./runtime-worker.js')
    worker.postMessage(
      {
        ...data.program,
        registry: loadRegistry(data.program.clientId),
        frameAcknowledgements: true,
        packedFrames: display.packedFrames,
        performanceEnabled: diagnostics.enabled,
      },
      [data.program.wasm.buffer, data.program.imageBytes.buffer],
    )
    diagnostics.program(data.program.clientId)
    const audioWorker = worker
    audio
      ?.connect()
      .then((port) => {
        if (audioWorker === worker && executing) worker.postMessage({ kind: 'audio', port }, [port])
        else port.close()
      })
      .catch((error) => {
        console.warn('Audio output unavailable:', error.message)
      })

    return
  }

  if (data.progress) {
    if (!executing) playStatus.textContent = data.progress

    return
  }

  if (data.finished) return finish()

  if (data.error) {
    console.error(data.stack ?? data.error)

    return finish(data.error, data.stack)
  }
}

function startWorker(source = './worker.js') {
  const instance = new Worker(source, { type: 'module' })

  instance.onmessage = (event) => {
    if (instance === worker) showResult(event)
  }

  instance.onerror = (event) => {
    if (instance === worker) finish(`Worker failed: ${event.message}`, event.error?.stack)
  }

  return instance
}

function finish(message = '', stack = '') {
  worker.terminate()
  audio?.stop()
  executing = false
  playButton.disabled = !directory && !selectedFiles.length
  playStatus.classList.remove('compiling')
  playStatus.textContent = message
  errorDetails.hidden = !message
  errorReport.hidden = true
  errorReport.value = message
    ? JSON.stringify(
        {
          message,
          stack,
          browser: navigator.userAgent,
          platform: navigator.platform,
          page: location.origin + location.pathname,
          date: new Date().toISOString(),
        },
        null,
        2,
      )
    : ''
  copyErrorStatus.textContent = ''
  launcher.hidden = false
  folderHelp.hidden = false
  gamePanel.hidden = true
  void resumeLauncherMusic()
}
