import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

// Reproduce Safari's nested FileReaderSync event dispatch deterministically,
// while the real wasm-bindgen Process.call(ReadFile) mutable borrow is live.
export async function reentrySmoke(Process, directory) {
  const { protectProcess, deferProcessTask } = await import(
    new URL('./runtime-access.js', directory)
  )
  const { ThreadHost } = await import(new URL('./threads.js', directory))
  const { createScheduler } = await import(new URL('./scheduler.js', directory))
  const fixtureProcess = () => {
    const memory = new WebAssembly.Memory({ initial: 8, maximum: 32 })
    const owner = new Process(memory, 0x10000, 0x200000)
    owner.adopt(0x1000, 4096, true)
    owner.adopt(0x10000, 0x40000, true)
    const api = (name) => JSON.parse(owner.resolve('kernel32.dll', name)).id
    const ids = Object.fromEntries(
      ['CreateFileA', 'ReadFile', 'Sleep'].map((name) => [name, api(name)]),
    )
    const call = (name, args) => owner.call(ids[name], Uint32Array.from(args))
    owner.addFileReader('nested.bin', 256 * 1024, (offset, length) => {
      reader()
      return new Uint8Array(length).fill(0x5a)
    })
    owner.write(0x1000, new TextEncoder().encode('C:\\Sims\\nested.bin\0'))
    const handle = call('CreateFileA', [0x1000, 0x80000000, 3, 0, 3, 0, 0])
    assert.notEqual(handle, 0xffffffff)
    const read = () => call('ReadFile', [handle, 0x10000, 256 * 1024, 0x1100, 0])
    const seek = JSON.parse(owner.resolve('kernel32.dll', 'SetFilePointer')).id
    const rewind = () => owner.call(seek, Uint32Array.of(handle, 0, 0, 0))
    return { owner, memory, ids, read, rewind }
  }
  const events = []
  let reader
  const positive = fixtureProcess()
  const unprotected = new ThreadHost(positive.owner)
  reader = () =>
    assert.throws(() => unprotected.drain(false), /recursive use of an object detected/)
  assert.equal(positive.read(), 1, 'the old host path reproduces the exact Rust alias error')
  await unprotected.close()
  // Throwing from wasm-bindgen's borrow guard also leaks an export's Rc clone;
  // use a separate owner so the fixed path must prove it frees successfully.
  assert.throws(() => positive.owner.free(), /ownership.*borrowed/)

  const { owner, memory, ids, read, rewind } = fixtureProcess()
  assert.equal(protectProcess(owner), owner)
  const wrappedCall = owner.call
  assert.equal(protectProcess(owner).call, wrappedCall, 'protection is installed once')
  const host = new ThreadHost(owner)
  const nativeTimeout = globalThis.setTimeout
  const nativeClearTimeout = globalThis.clearTimeout
  const nativeMicrotask = globalThis.queueMicrotask
  const previousNow = Object.getOwnPropertyDescriptor(performance, 'now')
  const previousSelf = Object.getOwnPropertyDescriptor(globalThis, 'self')
  const timers = new Map()
  const microtasks = []
  let nextTimer = 0
  let now = 100
  const pump = () => {
    const [id, run] = timers.entries().next().value
    timers.delete(id)
    run()
  }
  const settle = async () => {
    for (let turn = 0; turn < 100; turn++) {
      while (microtasks.length) microtasks.shift()()
      if (timers.size) pump()
      await Promise.resolve()
      await Promise.resolve()
      if (!timers.size && !microtasks.length) return
    }
    assert.fail('deferred work did not settle')
  }
  const temp = await mkdtemp(join(tmpdir(), 'sims-reentry-'))
  let runningWorker
  let finishWorker
  try {
    // Use production worker message handling with a controllable game lifetime.
    await writeFile(
      join(temp, 'guest.mjs'),
      `
      let owner, finish
      export function use(process) { owner = process }
      export function stop() { finish() }
      export function createFramePresenter() {
        return {setPacked() {}, acknowledge() { owner.takeFrame() }, close() {}}
      }
      export const describeGuestError = error => ({error: String(error), stack: error.stack})
      export async function runGuest(program, report, hooks) {
        hooks.onReady(owner)
        await new Promise(resolve => { finish = resolve })
        hooks.onReady(null)
      }
    `,
    )
    const source = await readFile(new URL('./runtime-worker.js', directory), 'utf8')
    await writeFile(
      join(temp, 'worker.mjs'),
      source.replace(
        /from '(\.\/[^']+)'/g,
        (_, name) =>
          `from '${['./guest.js', './frame-output.js'].includes(name) ? './guest.mjs' : new URL(name, directory).href}'`,
      ),
    )
    const fixture = await import(pathToFileURL(join(temp, 'guest.mjs')))
    fixture.use(owner)
    finishWorker = fixture.stop
    const messages = []
    Object.defineProperty(globalThis, 'self', {
      configurable: true,
      value: { postMessage: (message) => messages.push(message) },
    })
    await import(pathToFileURL(join(temp, 'worker.mjs')))
    runningWorker = self.onmessage({ data: { kind: 'run' } })
    const replies = []
    const audioPort = {
      postMessage: (reply) => replies.push(reply),
      close() {
        this.closed = true
      },
    }
    await self.onmessage({ data: { kind: 'audio', port: audioPort } })

    globalThis.setTimeout = (run) => {
      timers.set(++nextTimer, run)
      return nextTimer
    }
    globalThis.clearTimeout = (id) => timers.delete(id)
    globalThis.queueMicrotask = (run) => microtasks.push(run)
    Object.defineProperty(performance, 'now', { configurable: true, value: () => now })
    const waiting = host.schedule(ids.Sleep, Uint32Array.of(10))
    assert.ok(waiting instanceof Promise)
    waiting.then(() => events.push('wake'))
    const timeout = timers.get(host.timer)
    timers.delete(host.timer)
    const scheduler = createScheduler(host.signal, owner)
    const guest = new WebAssembly.Instance(
      new WebAssembly.Module(
        Uint8Array.from([
          0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 2, 12, 1, 3, 101, 110, 118, 4, 99, 97,
          108, 108, 0, 0, 3, 2, 1, 0, 7, 7, 1, 3, 114, 117, 110, 0, 1, 10, 6, 1, 4, 0, 16, 0, 11,
        ]),
      ),
      {
        env: {
          call() {
            owner.advanceTime(now)
            events.push('guest')
          },
        },
      },
    ).exports.run
    // Arm the runnable-thread microtask outside the read, then pump it inside.
    host.drain()
    const queued = microtasks.shift()
    let deferred
    let guestTask
    reader = () => {
      events.push('reader')
      now = 200
      timeout()
      queued()
      void self.onmessage({ data: { kind: 'mouse', x: 10, y: 20, buttons: 1 } })
      void self.onmessage({ data: { kind: 'key', code: 65, down: true, repeat: false } })
      void self.onmessage({ data: { kind: 'frame-ready' } })
      audioPort.onmessage({ data: { frames: 256, rate: 48000 } })
      guestTask = scheduler.run(guest)
      deferred = deferProcessTask(owner, () => {
        owner.setThread(1)
        events.push('tail')
      })
      assert.equal(timers.size, 0, 'deferred tasks cannot run before the outer Rust export returns')
      assert.equal(replies.length, 0, 'PCM is deferred together with its reply')
      assert.equal(host.waits.size, 1, 'the sleeping continuation stays suspended during ReadFile')
    }
    rewind()
    assert.equal(read(), 1)
    events.push('returned')
    assert.deepEqual(events, ['reader', 'returned'])
    assert.equal(new DataView(memory.buffer).getUint32(0x1100, true), 256 * 1024)
    assert.ok(new Uint8Array(memory.buffer, 0x10000, 256 * 1024).every((byte) => byte === 0x5a))
    assert.equal(timers.size, 1, 'one deferred task is armed after native return')
    await settle()
    await deferred
    await guestTask
    assert.equal(await waiting, 0)
    assert.deepEqual(events, ['reader', 'returned', 'wake', 'guest', 'tail'])
    assert.equal(host.signal.aborted, false, host.signal.reason?.stack)
    assert.equal(replies.length, 1, 'the worklet gets exactly one response')
    assert.ok(replies[0].left.every((sample) => sample === 0))
    assert.ok(replies[0].right.every((sample) => sample === 0))
    assert.equal(
      messages.length,
      0,
      'nested worker input/frame/audio events produce no alias error',
    )
    const user = (name, args) =>
      owner.call(JSON.parse(owner.resolve('user32.dll', name)).id, Uint32Array.from(args))
    assert.equal(user('GetAsyncKeyState', [65]) & 0x8000, 0x8000)
    assert.equal(user('GetCursorPos', [0x1180]), 1)
    assert.deepEqual([...new Int32Array(memory.buffer, 0x1180, 2)], [10, 20])

    // A queued flush can itself be pumped during a later native read. It must
    // retain its work and rearm only when that export also returns.
    rewind()
    reader = () => {
      deferred = deferProcessTask(owner, () => events.push('second'))
    }
    read()
    assert.equal(timers.size, 1)
    rewind()
    reader = () => {
      pump()
      assert.equal(timers.size, 0)
    }
    read()
    assert.equal(events.includes('second'), false)
    await settle()
    await deferred
    assert.equal(events.at(-1), 'second')

    // A previously suspended guest's pacing timer can be pumped as well. Its
    // Promise must not resolve until the reader and outer export have returned.
    scheduler.deferFrame(4)
    let resumed = false
    const pacing = scheduler.run(scheduler.checkpoint, 0x401000).then(() => {
      owner.advanceTime(now)
      resumed = true
    })
    const pacingTimer = timers.values().next().value
    timers.clear()
    rewind()
    reader = () => {
      pacingTimer()
      assert.equal(timers.size, 0)
      assert.equal(resumed, false)
    }
    read()
    assert.equal(resumed, false)
    await settle()
    await pacing
    assert.equal(resumed, true)

    // Deferred failures keep their identity and do not strand later events.
    const taskError = new Error('deferred task failed')
    let failedTask
    rewind()
    reader = () => {
      failedTask = deferProcessTask(owner, () => {
        throw taskError
      })
      failedTask.catch(() => {})
      deferred = deferProcessTask(owner, () => owner.setThread(1))
    }
    read()
    await settle()
    await assert.rejects(failedTask, (error) => error === taskError)
    await deferred

    // A normal Rust exception must release the host gate without altering it.
    assert.throws(() => owner.read(0, 1), /not mapped|invalid|range/i)
    assert.equal(
      deferProcessTask(owner, () => 42),
      42,
    )

    // Closing during a nested event aborts immediately, but cancels native waits
    // and awaits queued cleanup before the process can be freed.
    now = 300
    const cancelled = host.schedule(ids.Sleep, Uint32Array.of(10))
    const reason = new Error('stop while reading')
    cancelled.catch(() => {})
    rewind()
    let closed
    reader = () => {
      audioPort.onmessage({ data: { frames: 256, rate: 48000 } })
      host.abort(reason)
      assert.equal(host.abortGlobal.value, 1)
      closed = host.close()
      audioPort.onmessage = null
      finishWorker()
    }
    read()
    await runningWorker
    await settle()
    await closed
    await assert.rejects(cancelled, (error) => error === reason)
    assert.equal(replies.length, 1, 'closing a port discards its deferred audio request')
    assert.equal(audioPort.closed, true)
    assert.equal(host.waits.size, 0)
    assert.equal(messages.filter((message) => message.error).length, 0)
    console.log(
      'Nested file-read events: exact Rust alias reproduced; timers, runnable threads, input, audio, FIFO and shutdown deferred safely',
    )
  } finally {
    finishWorker?.()
    await runningWorker
    host.abort(new Error('Reentry test stopped.'))
    await settle()
    globalThis.setTimeout = nativeTimeout
    globalThis.clearTimeout = nativeClearTimeout
    globalThis.queueMicrotask = nativeMicrotask
    if (previousNow) Object.defineProperty(performance, 'now', previousNow)
    else delete performance.now
    if (previousSelf) Object.defineProperty(globalThis, 'self', previousSelf)
    else delete globalThis.self
    await host.close()
    owner.free()
    await rm(temp, { recursive: true, force: true })
  }
}
