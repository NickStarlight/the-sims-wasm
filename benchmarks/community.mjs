import { chromium } from 'playwright'
import { PNG } from 'pngjs'
import fs from 'node:fs'
// Install benchmark dependencies with npm install --prefix benchmarks.
// Usage: node benchmarks/community.mjs LABEL GAME_FOLDER [CDP_URL] [SITE_URL]
const label = process.argv[2] || 'run'
const profiling = process.env.SIMS_PROFILE === '1'
if (!process.argv[3]) throw new Error('Supply the game folder')
const browser = await chromium.connectOverCDP(process.argv[4] || 'http://127.0.0.1:9229')
const context = await browser.newContext({
  viewport: { width: 1280, height: 960 },
  serviceWorkers: 'block',
})
const page = await context.newPage()
const errors = []
page.on('pageerror', (e) => errors.push(String(e)))
try {
  await page.goto(process.argv[5] || 'http://127.0.0.1:8091')
  await page.locator('input[webkitdirectory]').setInputFiles(process.argv[3])
  const start = Date.now()
  await page.locator('button.play').click()
  async function untilPixel(x, y, match) {
    const deadline = Date.now() + 120000
    while (Date.now() < deadline) {
      const p = PNG.sync.read(await page.screenshot())
      const i = (y * p.width + x) * 4
      if (match(...p.data.subarray(i, i + 3))) return Date.now()
      await page.waitForTimeout(250)
    }
    throw Error('Scene timeout')
  }
  const neighborhood = await untilPixel(1000, 15, (r, g, b) => b > r * 2 && b > g * 1.5)
  await page.screenshot({ path: `/tmp/${label}-neighborhood.png` })
  await page.mouse.move(940, 720)
  await page.waitForTimeout(300)
  const lotClick = Date.now()
  await page.mouse.click(940, 720)
  const lot = await untilPixel(150, 850, (r, g, b) => b > 100 && g < 60 && r < 30)
  await page.screenshot({ path: `/tmp/${label}-lot.png` })
  await page.keyboard.press('Alt+Shift+P')
  await page.evaluate(() => {
    navigator.clipboard.writeText = async (text) => {
      window.benchmarkReport = text
    }
  })
  let profiler
  if (profiling) {
    const cdp = await browser.newBrowserCDPSession()
    const { targetInfos } = await cdp.send('Target.getTargets')
    const target = targetInfos.find(
      (t) => t.type === 'worker' && t.url.endsWith('/runtime-worker.js'),
    )
    if (!target) throw new Error('Runtime worker missing')
    const { sessionId } = await cdp.send('Target.attachToTarget', {
      targetId: target.targetId,
      flatten: false,
    })
    let id = 0
    const pending = new Map()
    cdp.on('Target.receivedMessageFromTarget', (event) => {
      if (event.sessionId !== sessionId) return
      const result = JSON.parse(event.message)
      const request = pending.get(result.id)
      if (request) {
        clearTimeout(request.timeout)
        pending.delete(result.id)
        result.error
          ? request.reject(Error(JSON.stringify(result.error)))
          : request.resolve(result.result)
      }
    })
    const send = (method, params = {}) =>
      new Promise((resolve, reject) => {
        const request = ++id
        const timeout = setTimeout(() => {
          pending.delete(request)
          reject(Error('Profiler command timed out: ' + method))
        }, 15000)
        pending.set(request, { resolve, reject, timeout })
        cdp
          .send('Target.sendMessageToTarget', {
            sessionId,
            message: JSON.stringify({ id: request, method, params }),
          })
          .catch((error) => {
            clearTimeout(timeout)
            pending.delete(request)
            reject(error)
          })
      })
    await send('Profiler.enable')
    await send('Profiler.setSamplingInterval', { interval: 1000 })
    await send('Profiler.start')
    const main = await context.newCDPSession(page)
    await main.send('Profiler.enable')
    await main.send('Profiler.start')
    profiler = async () => {
      const worker = await send('Profiler.stop')
      fs.writeFileSync(`/tmp/${label}-worker.cpuprofile`, JSON.stringify(worker.profile))
      const result = await main.send('Profiler.stop')
      fs.writeFileSync(`/tmp/${label}-page.cpuprofile`, JSON.stringify(result.profile))
      await cdp.detach()
    }
  }
  await page.locator('#performance-reset').click()
  await page.locator('#performance-hide').click()
  for (let i = 0; i < 8; i++) {
    await page.mouse.move(640, 400)
    await page.mouse.down({ button: 'right' })
    await page.waitForTimeout(100)
    for (let step = 1; step <= 30; step++) {
      await page.mouse.move(640 + ((i % 2 ? -1 : 1) * 200 * step) / 30, 400)
      await page.waitForTimeout(16)
    }
    await page.waitForTimeout(1300)
    await page.mouse.up({ button: 'right' })
  }
  if (profiler) await profiler()
  await page.locator('#performance-toggle').click()
  await page.locator('#performance-copy').click()
  const report = JSON.parse(await page.evaluate(() => window.benchmarkReport))
  Object.assign(report, {
    label,
    profiling,
    browserVersion: browser.version(),
    viewport: page.viewportSize(),
    playToNeighborhoodMs: neighborhood - start,
    lotLoadMs: lot - lotClick,
    errors,
  })
  fs.writeFileSync(`/tmp/${label}.json`, JSON.stringify(report, null, 2))
  await page.screenshot({ path: `/tmp/${label}-pan.png` })
  console.log(
    JSON.stringify({
      label,
      playToNeighborhoodMs: neighborhood - start,
      lotLoadMs: lot - lotClick,
      fps: report.measurement.gameFramesPerSecond,
      frame: report.measurement.frameIntervalMs,
      errors,
    }),
  )
} finally {
  await context.close()
  await browser.close()
}
