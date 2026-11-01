import { GameSession } from './game-session.js'
import { Launcher } from './launcher.js'

const launcher = new Launcher()
const session = new GameSession({ onFinish: launcher.show })

launcher.onPlay({
  prepare: session.resumeAudio,
  start: session.start,
})
