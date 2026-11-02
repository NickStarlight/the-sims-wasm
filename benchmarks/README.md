# Real-game measurements

Install harness dependencies with `npm install --prefix benchmarks`. Build and serve
with `cargo xtask build-web` and `cargo xtask serve --port 8091`. Start a separate,
headed Chrome with a remote debugging port and a disposable profile. Do not run
other games, builds, or tests during measurements.

```sh
node benchmarks/community.mjs baseline-1 /path/to/Sims http://127.0.0.1:9229
SIMS_PROFILE=1 node benchmarks/community.mjs profile /path/to/Sims http://127.0.0.1:9229
python benchmarks/summarize-profile.py /tmp/profile-worker.cpuprofile
python benchmarks/summarize-profile.py /tmp/profile-page.cpuprofile
python benchmarks/summarize-runs.py /tmp/baseline-*.json -- /tmp/candidate-*.json
```

The scenario assumes Neighborhood 1's community park and a 1280×960 viewport.
It opens a fresh browser context, selects the supplied game files, waits for the
neighborhood, enters the park, and performs eight paced right-button camera drags.
Reports and screenshots are written to `/tmp/LABEL*`; profiled runs also produce
Chrome `.cpuprofile` files. Keep profiling separate from timing comparisons.
Scene detection polls screenshots every 250 ms, so load times include polling and
capture uncertainty. The performance report describes the trailing 15-second
window. Use at least three fresh runs per build and compare medians and ranges.
Screenshots must be inspected to ensure the expected scene loaded without prompts.

The profile summaries report CPU **self time**, not inclusive stacks or GPU time.
They are useful for locating costs, not for claiming a frame-rate improvement.

## Camera-pan investigation (2026-11-01)

Baseline: `17e372f`, Chrome 156 on Linux, AMD RX 7900 XTX, original game executable
SHA-256 `6e598c22512a24659cbd0e533306abf3129733d67b1d120916b1b6b1581e29f2`.
The six `pan-{baseline,candidate}-N.json` files contain the unprofiled measurements.
Three sequential baseline runs were followed by three candidate runs on the same
machine, without concurrent builds or other game sessions.

| Median of three runs | Baseline | Candidate |
| --- | ---: | ---: |
| Play → neighborhood | 40.708 s | 40.809 s |
| Community lot load | 2.612 s | 2.636 s |
| Camera-pan FPS | 36.80 | 36.67 |
| Frame interval p95 | 61.8 ms | 62.4 ms |

All six runs completed without page errors and reached the community park.
These measurements show no useful improvement; they do not establish a
statistically significant regression either. Load-time differences are smaller
than the scene-polling interval.

The rejected candidate extended dead x86 flag elimination past nonfaulting,
flag-transparent register instructions. It stopped at memory accesses, flag
consumers, control transfers and cooperative checkpoints. The implementation and
regression tests are preserved in `experiments/dead-flags.patch`, applicable to the
baseline with `git apply`. It passed `cargo xtask check --browser`, including carry
and memory-fault flag checks, but was removed after the real-game measurements.
That experiment left the production compiler unchanged.

The separate CPU profile attributed 78.56% of worker sample time to translated
game functions and 3.99% to the compatibility runtime. The page thread was 98.52%
idle. The largest named guest cost was `guest_00498830` (alpha compositor), followed
by several other guest functions; no single function dominated the whole trace.
This points toward targeted guest-code work for the next performance experiment.
It does not establish GPU utilization or exclude GPU stalls.

`pan-{worker,page}-profile.json` summarize the raw captures in `profiles/`.
Decompress a `.cpuprofile.gz` with `gzip -dc` and load the resulting `.cpuprofile`
in Chrome DevTools to inspect complete stacks. Profiled timings were excluded
from the comparison above. The two Python utilities use the standard library;
the browser harness uses the existing Playwright and pngjs dependencies.

## Residential smoke test

On the restored baseline build, in a disposable browser context, opened the Goth
family at 5 Sim Lane (neighborhood click 1040,330). Dismissed the expansion/help
prompts, observed autonomous simulation advance from 6:32 to 6:42, and paused.
Bought the first dining chair for $80 and placed it on the lawn near screen
800,550. In Build mode, placed two $3 floor tiles under/beside it. The household
balance changed from $2,542 to $2,456. Saved through the game's floppy-disk button;
the OPFS journal was 1,599,161 bytes. Quit through the game's own Quit dialog,
reloaded the browser page, reselected the same source files, and reopened the lot.

Verified visually that the chair, both floor tiles, $2,456 balance, 6:42 saved time,
and queued bookcase interaction returned. Unpaused and observed movement and time
advance to 6:47. No page errors were reported. Original game files were supplied
as read-only browser File objects; changes lived in this context's OPFS storage.

The introductory expansion/help dialogs reappeared after reload. This test does
not establish whether that matches native-game behavior; household persistence
passed independently. It is a short smoke test, not a full simulation/compatibility
certification. Local screenshots are `/tmp/residential-saved.png`,
`/tmp/residential-restored-final.png`, and `/tmp/residential-resumed-final.png`.

Validation: `cargo xtask check --browser` passed with the experimental compiler;
`npm run format:check` passed. The final profiling harness completed a separate
real-game run without page errors and captured both CPU profiles. Archived profile
summaries were regenerated and matched. The comparison utility correctly rejects
reports marked as profiled. `git diff --check` and the rejected patch's
`git apply --check` passed. No production source changes were retained from that experiment.

## Retained compositor optimization (2026-11-01)

The alpha classification loop in the fingerprinted `guest_00498830` routine now
batches a validated row in Wasm. It computes the same alpha words and blend/opaque
masks, checks bounds and aliases before making changes, preserves the original
translated register state, and never crosses an observable checkpoint. Inputs
that fail a guard use the original instruction translation. Recognition still
requires the existing complete-routine fingerprint; other executable versions
continue through the ordinary compiler.

| Median | Baseline control (3 runs) | Optimized (6 runs) |
| --- | ---: | ---: |
| Camera-pan FPS | 36.33 | 37.90 |
| Frame interval p95 | 61.3 ms | 61.8 ms |
| Play → neighborhood | 40.316 s | 40.193 s |
| Community lot load | 2.568 s | 2.556 s |

The measured FPS gain is **4.32%** in this scenario. There is no demonstrated
improvement in p95 frame time or loading speed. The isolated 100,000-row benchmark
improved from 54.91 to 20.87 ms (2.63×), which is not a whole-game speedup.

The comparison order was three optimized runs, three baseline control runs, then
three optimized confirmation runs. The confirmation batch's median was 37.80 FPS.
All nine runs completed without page errors. The fixed park region (793,600 RGB
pixels, excluding the pause label and bottom panel) matched exactly across all
nine captures. The baseline control runs and both optimized batches used the same
browser session and matched rendering layout. Initial copy-experiment runs used
an earlier browser-context layout and are kept separately from this comparison.
This is still a small sample from one machine and one scene.

Before this, a previous-color word-copy specialization passed its differential
tests and ran about twice as fast in isolation, but the game measurements regressed
(37.20 → 35.60 median FPS). It was removed; `experiments/compositor-copy.patch`
preserves the experiment against `17e372f`. An early classification prototype
miscounted overlapping blocks; it was corrected before the retained measurements.
Its timings are excluded.

Correctness coverage includes 328 differential cases against the original
translation: alpha thresholds, signed wrapping offsets, partial rows, aliasing,
invalid memory, checkpoint budgets, callback mutations, and interruptions.
Independent assertions verify alpha values and masks. A separate discovery test
pins the overlapping opaque/update block so checkpoint counts and final trap
markers match the actual translator. `cargo xtask check --browser` passed with
304 Rust tests and the browser/runtime suites.

Raw reports, microbenchmarks, source hashes, comparisons, and limits are under
`compositor-2026-11-01/`. To reproduce the isolated benchmark and screenshot check:

```sh
SIMS_CLASSIFY_BENCH_REPORT=/tmp/classify-micro.json cargo test -p sims-compiler --lib alpha_classification
node benchmarks/compare-scenes.mjs /tmp/control-lot.png /tmp/candidate-lot.png
```
