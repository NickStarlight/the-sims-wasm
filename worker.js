import { loadGame } from './game.js'

self.onmessage = async ({ data }) => {
  if (data?.kind !== 'run') return

  try {
    const program = await loadGame(
      data.files,
      (phase) => self.postMessage({ phase }),
      (progress) => self.postMessage({ progress }),
    )
    self.postMessage({ program }, [program.imageBytes.buffer])
  } catch (error) {
    self.postMessage({ error: error.message })
  }
}
