// Run with Playwright available, against a dedicated Chrome CDP session.
// First open Neighborhood 1's community park. This does not save the lot.
// Usage: node camera-pan.mjs http://127.0.0.1:9229 output.json
import { chromium } from 'playwright'
import { writeFile } from 'node:fs/promises'
const browser = await chromium.connectOverCDP(process.argv[2])
try {
  const page = browser.contexts()[0].pages()[0]
  await page.evaluate(() => {
    navigator.clipboard.writeText = async (text) => {
      window.benchmarkReport = text
    }
  })
  if (await page.locator('#performance-panel').isHidden()) await page.keyboard.press('Alt+Shift+P')
  await page.locator('#performance-reset').click()
  await page.locator('#performance-hide').click()
  for (let i = 0; i < 8; i++) {
    await page.mouse.move(640, 400)
    await page.mouse.down({ button: 'right' })
    await page.mouse.move(i % 2 ? 440 : 840, 400, { steps: 30 })
    await page.waitForTimeout(1800)
    await page.mouse.up({ button: 'right' })
  }
  await page.locator('#performance-toggle').click()
  await page.locator('#performance-copy').click()
  await writeFile(process.argv[3], await page.evaluate(() => window.benchmarkReport))
  await page.screenshot({ path: process.argv[3] + '.png' })
} finally {
  await browser.close()
}
