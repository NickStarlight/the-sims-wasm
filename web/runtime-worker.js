import { createFramePresenter } from './frame-output.js'
import { describeGuestError, runGuest } from './guest.js'
import { serveAudio } from './audio-output.js'
import { deferProcessTask } from './runtime-access.js'

let owner
let closeAudio
let presented = false

const sendFrame = (frame) => {
  if (!presented) {
    presented = true
    self.postMessage({ progress: `First game frame received (${frame.width}×${frame.height}).` })
  }

  self.postMessage({ frame }, [frame.pixels.buffer])
}

const frames = createFramePresenter(sendFrame)

function scheduleHostTask(run) {
  const process = owner

  try {
    const pending = deferProcessTask(process, () => {
      if (owner === process) run()
    })
    pending?.catch((error) => self.postMessage(describeGuestError(error)))
  } catch (error) {
    self.postMessage(describeGuestError(error))
  }
}

self.onmessage = async ({ data }) => {
  if (data.kind === 'frame-ready') {
    scheduleHostTask(() => frames.acknowledge())

    return
  }

  if (data.kind === 'audio') {
    closeAudio?.()
    closeAudio = serveAudio(
      data.port,
      () =>
        owner
          ? (left, right, rate) => {
              const block = owner.mixAudio(left.length, rate, performance.now())
              let failed = true

              try {
                left.set(block.left())
                right.set(block.right())
                failed = false
              } finally {
                try {
                  block.free()
                } catch (error) {
                  if (!failed) throw error
                }
              }
            }
          : undefined,
      scheduleHostTask,
    )

    return
  }

  if (data.kind === 'mouse') {
    scheduleHostTask(() => owner?.mouseInput(data.x, data.y, data.buttons))

    return
  }

  if (data.kind === 'key') {
    scheduleHostTask(() => owner?.keyInput(data.code, data.down, data.repeat))

    return
  }

  try {
    frames.setPacked(data.packedFrames)
    await runGuest(data, (progress) => self.postMessage({ progress }), {
      onReady(process) {
        owner = process

        if (!process) {
          frames.close()
          closeAudio?.()
          closeAudio = undefined
        }
      },
      onRegistry(registry) {
        self.postMessage({ registry, clientId: data.clientId })
      },
      onPhase(phase) {
        self.postMessage({ phase })
      },
      onFrame: sendFrame,
      presenter: data.frameAcknowledgements ? frames : undefined,
    })
    self.postMessage({ finished: 'Guest stopped.' })
  } catch (error) {
    self.postMessage(describeGuestError(error))
  }
}
