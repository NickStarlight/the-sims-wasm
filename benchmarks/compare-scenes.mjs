// Usage: node benchmarks/compare-scenes.mjs BASELINE_LOT.png CANDIDATE_LOT.png [...]
import { PNG } from 'pngjs'
import fs from 'node:fs'
if (process.argv.length < 4) throw Error('Supply baseline and candidate lot screenshots')
const compare = (left, right) => {
  const a = PNG.sync.read(fs.readFileSync(left)),
    b = PNG.sync.read(fs.readFileSync(right))
  if ([a, b].some((p) => p.width !== 1280 || p.height !== 960))
    throw Error('Expected 1280x960 community benchmark screenshots')
  let changed = 0,
    pixels = 0,
    maxChannelDifference = 0
  // Exclude the blinking pause label and the controls. Compare the fixed park view.
  for (let y = 160; y < 780; y++)
    for (let x = 0; x < a.width; x++) {
      const i = (y * a.width + x) * 4
      let different = false
      for (let c = 0; c < 3; c++) {
        const d = Math.abs(a.data[i + c] - b.data[i + c])
        different ||= d !== 0
        maxChannelDifference = Math.max(maxChannelDifference, d)
      }
      changed += Number(different)
      pixels++
    }
  return {
    left,
    right,
    region: { x: 0, y: 160, width: a.width, height: 620 },
    pixels,
    changed,
    percentChanged: (100 * changed) / pixels,
    maxChannelDifference,
  }
}
console.log(
  JSON.stringify(
    process.argv.slice(3).map((p) => compare(process.argv[2], p)),
    null,
    2,
  ),
)
