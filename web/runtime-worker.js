import { createFramePresenter } from './frame-output.js'
import { runGuest } from './guest.js'
import { serveAudio } from './audio-output.js'

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

self.onmessage = async ({ data }) => {
  if (data.kind === 'frame-ready') {
    frames.acknowledge()

    return
  }

  if (data.kind === 'audio') {
    closeAudio?.()
    closeAudio = serveAudio(data.port, () =>
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
    )

    return
  }

  if (data.kind === 'mouse') {
    owner?.mouseInput(data.x, data.y, data.buttons)

    return
  }

  if (data.kind === 'key') {
    owner?.keyInput(data.code, data.down, data.repeat)

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
    self.postMessage({ error: error.message, stack: error.stack })
  }
}
