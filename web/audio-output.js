/** Page-owned AudioContext; PCM travels directly from the game worker to the
 * audio worklet over a transferred port. No guest RAM crosses this boundary. */

export function serveAudio(port, render) {
  port.onmessage = (e) => {
    const { frames, rate } = e.data ?? {}

    if (
      !Number.isInteger(frames) ||
      frames < 1 ||
      frames > 4096 ||
      !Number.isFinite(rate) ||
      rate < 8000 ||
      rate > 192000
    )
      return

    const left = new Float32Array(frames)
    const right = new Float32Array(frames)
    render()?.(left, right, rate)
    port.postMessage({ left, right }, [left.buffer, right.buffer])
  }

  return () => {
    port.onmessage = null
    port.close()
  }
}

// Loaded through a Blob URL so the worklet needs no separate file fetch.
// https://developer.mozilla.org/en-US/docs/Web/API/AudioWorkletProcessor/process
export const AUDIO_PROCESSOR_SOURCE = `
class SimsAudio extends AudioWorkletProcessor {
  constructor() {
    super()
    this.queue = []
    this.offset = 0
    this.pending = false
    this.link = null

    this.port.onmessage = (e) => {
      if (e.data.stop) {
        if (this.link) this.link.close()

        this.link = null
        this.stopped = true

        return
      }

      if (!e.data.port) return

      if (this.link) this.link.close()

      this.link = e.data.port
      this.queue = []
      this.offset = 0
      this.pending = false

      this.link.onmessage = (e) => {
        this.pending = false
        const { left, right } = e.data

        if (
          left instanceof Float32Array &&
          right instanceof Float32Array &&
          left.length === 2048 &&
          right.length === 2048 &&
          this.queue.length < 2
        )
          this.queue.push([left, right])
      }
    }
  }

  process(inputs, outputs) {
    if (this.stopped) return false

    const out = outputs[0]

    if (!out || out.length < 2) return true

    let written = 0

    while (written < out[0].length && this.queue.length) {
      const block = this.queue[0]
      const n = Math.min(out[0].length - written, block[0].length - this.offset)

      for (let ch = 0; ch < 2; ch++)
        out[ch].set(block[ch].subarray(this.offset, this.offset + n), written)

      this.offset += n
      written += n

      if (this.offset === block[0].length) {
        this.queue.shift()
        this.offset = 0
      }
    }

    // At most one request in flight and two blocks queued (under 86ms at48kHz).
    if (this.link && !this.pending && this.queue.length < 2) {
      this.pending = true
      this.link.postMessage({ frames: 2048, rate: sampleRate })
    }

    return true
  }
}
registerProcessor('sims-audio', SimsAudio)
`

export class BrowserAudio {
  context = new AudioContext({ latencyHint: 'interactive' })
  node
  loaded
  /** Resume during the Play gesture so browser autoplay policy allows audio. */
  resume() {
    void this.context.resume().catch(() => {})
  }

  async connect() {
    this.loaded ??= (async () => {
      const url = URL.createObjectURL(
        new Blob([AUDIO_PROCESSOR_SOURCE], { type: 'text/javascript' }),
      )

      try {
        await this.context.audioWorklet.addModule(url)
      } finally {
        URL.revokeObjectURL(url)
      }
    })()
    await this.loaded
    this.stop()
    const channel = new MessageChannel()
    this.node = new AudioWorkletNode(this.context, 'sims-audio', {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [2],
    })
    this.node.port.postMessage({ port: channel.port1 }, [channel.port1])
    this.node.connect(this.context.destination)

    return channel.port2
  }

  stop() {
    this.node?.port.postMessage({ stop: true })
    this.node?.disconnect()
    this.node?.port.close()
    this.node = undefined
  }
}
