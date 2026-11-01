export class LauncherMusic {
  constructor(audio, isVisible) {
    this.audio = audio
    this.isVisible = isVisible
    this.finished = false
    this.audio.volume = 0.45
    this.ready = this.load()

    audio.addEventListener('ended', () => {
      this.finished = true
    })
    document.addEventListener('click', this.onGesture)
    void this.resume()
  }

  async load() {
    const response = await fetch(this.audio.getAttribute('src'))

    if (!response.ok) throw new Error('Launcher music could not be loaded.')

    this.audio.src = URL.createObjectURL(await response.blob())
  }

  async resume() {
    try {
      await this.ready

      if (this.finished || !this.isVisible()) return

      await this.audio.play()

      if (this.finished || !this.isVisible()) this.audio.pause()
    } catch (error) {
      // Autoplay may need the first user gesture. Never restart a finished clip.
      if (error.name !== 'NotAllowedError' && error.name !== 'AbortError') {
        this.finished = true
      }
    }
  }

  onGesture = () => {
    if (!this.finished && this.isVisible() && this.audio.paused) void this.resume()
  }

  stop() {
    this.finished = true
    this.audio.pause()
  }
}
