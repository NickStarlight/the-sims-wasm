import assert from 'node:assert/strict'
import { runInNewContext } from 'node:vm'

export async function nativeImportsSmoke(Process, directory) {
  const { ApiDispatcher } = await import(new URL('./dispatch.js', directory))
  const { GuestThreadExit } = await import(new URL('./scheduler.js', directory))
  const { installSynchronousImports } = await import(new URL('./sync-imports.js', directory))
  const { installNativeImports } = await import(new URL('./native-imports.js', directory))
  const { ThreadHost } = await import(new URL('./threads.js', directory))
  let passed = 0
  function scenario(native, name = 'GetLastError', options = {}) {
    const memory = options.memory ?? new WebAssembly.Memory({ initial: 4, maximum: 64 })
    const owner = options.owner ?? new Process(memory, 0x10000, 0x400000)
    const stack = owner.heapAlloc(64),
      pointer = owner.heapAlloc(64)
    if (!options.owner)
      owner.call(JSON.parse(owner.resolve('kernel32.dll', 'TlsAlloc')).id, new Uint32Array())
    const registers = Object.fromEntries(
      ['esp', 'eax', 'actual_return'].map((key) => [
        key,
        new WebAssembly.Global({ value: 'i32', mutable: true }, 0),
      ]),
    )
    const reports = []
    const report = (message) => {
      reports.push({ message, registers: snapshot() })
      if (options.reportError) throw new Error('report failure')
    }
    const host = new ApiDispatcher(
      { process: owner, memory, registers },
      { yields: 0 },
      report,
      () => {},
    )
    host.threadId = 7
    host.calls = 1000
    const cancellation = new AbortController()
    let aborted = false
    cancellation.signal.addEventListener('abort', () => {
      aborted = true
    })
    host.threads = { signal: cancellation.signal, notify() {} }
    host.checkAbort = () => {
      if (aborted) cancellation.signal.throwIfAborted()
    }
    const table = new WebAssembly.Table({ initial: 512, element: 'anyfunc' })
    const dll = options.dll ?? 'kernel32.dll'
    const api = { ...JSON.parse(owner.resolve(dll, name)), ...options.api }
    const binding = { label: options.label ?? dll + '!' + name, api }
    const imports = { trap: 0, bindings: [binding] }
    host.bindings = imports.bindings
    installSynchronousImports(table, imports, (slot) => host.dispatchSync(slot))
    const previous = table.get(16)
    const installed = native ? installNativeImports(table, imports, host) : null
    function snapshot() {
      return Object.fromEntries(
        Object.entries(registers).map(([key, value]) => [key, value.value >>> 0]),
      )
    }
    function prepare(args = [], ret = 0xffffffff) {
      registers.esp.value = stack
      registers.eax.value = 0x33
      registers.actual_return.value = 0x77
      const view = new DataView(memory.buffer)
      view.setUint32(stack, ret, true)
      args.forEach((value, i) => view.setUint32(stack + 4 + i * 4, value, true))
    }
    function run() {
      return table.get(16)()
    }
    function close() {
      cancellation.abort(new Error('Test execution closed'))
      if (!options.owner) owner.free()
    }
    return {
      memory,
      owner,
      stack,
      pointer,
      registers,
      host,
      cancellation,
      table,
      api,
      imports,
      reports,
      prepare,
      run,
      close,
      snapshot,
      installed,
      previous,
    }
  }
  const outcome = (fn) => {
    try {
      fn()
      return null
    } catch (error) {
      return { name: error.name, message: error.message }
    }
  }
  function pair(name, args, options = {}, setup = () => {}, verify = () => {}) {
    const before = scenario(false, name, options),
      after = scenario(true, name, options)
    try {
      for (const p of [before, after]) {
        setup(p)
        p.prepare(
          args.map((value) =>
            typeof value === 'function'
              ? value(p)
              : value === 'pointer'
                ? p.pointer
                : value === 'stack'
                  ? p.stack
                  : value,
          ),
        )
      }
      const afterError = outcome(after.run),
        beforeError = outcome(before.run)
      assert.deepEqual(afterError, beforeError, name + ' error parity')
      assert.deepEqual(after.snapshot(), before.snapshot(), name + ' register parity')
      assert.equal(after.host.calls, before.host.calls)
      assert.equal(after.host.lastCall, before.host.lastCall)
      assert.deepEqual(after.reports, before.reports)
      assert.deepEqual(
        [...after.owner.read(after.pointer, 64)],
        [...before.owner.read(before.pointer, 64)],
      )
      assert.deepEqual(
        [...after.owner.read(after.stack, 64)],
        [...before.owner.read(before.stack, 64)],
      )
      assert.equal(after.memory.buffer.byteLength, before.memory.buffer.byteLength)
      verify(before, beforeError)
      verify(after, afterError)
      passed++
    } finally {
      before.close()
      after.close()
    }
  }
  for (const cdecl of [false, true]) {
    const cases = [
      ['GetVersion', []],
      ['SetLastError', [0xffffffff]],
      ['TlsSetValue', [0, 0xffffffff]],
      ['InterlockedCompareExchange', ['pointer', 0xeeeeeeee, 0xffffffff]],
      ['VirtualAlloc', [0x30000, 0x20000, 0x3000, 4]],
    ]
    for (const [name, args] of cases)
      pair(name, args, cdecl ? { api: { stackArgs: 0 } } : {}, (p) =>
        new DataView(p.memory.buffer).setUint32(p.pointer, 0xffffffff, true),
      )
  }
  pair('InterlockedExchange', ['pointer', 0], {}, (p) =>
    new DataView(p.memory.buffer).setUint32(p.pointer, 0xffffffff, true),
  )
  pair('InterlockedIncrement', [0])
  pair('InterlockedIncrement', [0xffffffff])
  pair('GetLastError', [], { api: { id: 0xffffffff } })
  pair('GetLastError', [], {}, (p) => {
    p.host.threadId = 0
  })
  const geometry = { dll: 'user32.dll' }
  const rect = (p, at, values) => {
    const view = new DataView(p.memory.buffer)
    values.forEach((value, i) => view.setUint32(at + i * 4, value, true))
  }
  const readRect = (p, at) => {
    const view = new DataView(p.memory.buffer)
    return Array.from({ length: 4 }, (_, i) => view.getUint32(at + i * 4, true))
  }
  const right = (p) => p.pointer + 16
  const seedRects = (p) => {
    rect(p, p.pointer, [0, 0, 10, 10])
    rect(p, right(p), [5, -5, 15, 8])
  }
  const geometryResult =
    (result, values, output = (p) => p.pointer) =>
    (p, error) => {
      assert.equal(error, null)
      if (p.installed !== null)
        assert.equal(p.installed, 1, p.imports.bindings[0].label + ' uses native trampoline')
      assert.equal(p.registers.eax.value >>> 0, result)
      if (values)
        assert.deepEqual(
          readRect(p, output(p)),
          values.map((v) => v >>> 0),
        )
    }
  for (const [name, values] of [
    ['OffsetRect', [0x7fffffff, 0xffffffff, 0x7ffffffe, 0]],
    ['InflateRect', [0x80000001, 0xffffffff, 0x7ffffffe, 0]],
  ])
    pair(
      name,
      ['pointer', 0xffffffff, 0x80000000],
      geometry,
      (p) => rect(p, p.pointer, [0x80000000, 0x7fffffff, 0x7fffffff, 0x80000000]),
      geometryResult(1, values),
    )
  pair(
    'IntersectRect',
    ['pointer', 'pointer', right],
    geometry,
    seedRects,
    geometryResult(1, [5, 0, 10, 8]),
  )
  pair(
    'UnionRect',
    [right, 'pointer', right],
    geometry,
    seedRects,
    geometryResult(1, [0, -5, 15, 10], right),
  )
  pair(
    'IntersectRect',
    ['pointer', 'pointer', 'pointer'],
    geometry,
    seedRects,
    geometryResult(1, [0, 0, 10, 10]),
  )
  pair(
    'IntersectRect',
    ['pointer', 'pointer', right],
    geometry,
    (p) => {
      seedRects(p)
      rect(p, right(p), [10, 0, 20, 10])
    },
    geometryResult(0, [0, 0, 0, 0]),
  )
  pair(
    'UnionRect',
    ['pointer', 'pointer', right],
    geometry,
    (p) => {
      rect(p, p.pointer, [3, 4, 3, 5])
      rect(p, right(p), [9, 9, 8, 8])
    },
    geometryResult(0, [0, 0, 0, 0]),
  )
  pair('IsRectEmpty', ['pointer'], geometry, seedRects, geometryResult(0))
  pair(
    'IsRectEmpty',
    ['pointer'],
    geometry,
    (p) => rect(p, p.pointer, [0x7fffffff, 0, 0x80000000, 1]),
    geometryResult(1),
  )
  pair(
    'OffsetRect',
    ['pointer', 1, 1],
    { dll: 'USER32.DLL', api: { stackArgs: 0 } },
    seedRects,
    (p, error) => {
      geometryResult(1, [1, 1, 11, 11])(p, error)
      assert.equal(p.registers.esp.value >>> 0, p.stack + 4, 'cdecl leaves arguments for caller')
    },
  )
  for (const [x, y, expected] of [
    [0, 0, 1],
    [9, 9, 1],
    [10, 9, 0],
    [9, 10, 0],
    [-1, 0, 0],
  ])
    pair('PtInRect', ['pointer', x, y], geometry, seedRects, geometryResult(expected))
  for (const name of [
    'OffsetRect',
    'InflateRect',
    'IntersectRect',
    'UnionRect',
    'IsRectEmpty',
    'PtInRect',
  ]) {
    const arity = name === 'IsRectEmpty' ? 1 : 3
    pair(
      name,
      Array(arity).fill(0),
      geometry,
      () => {},
      geometryResult(name === 'IsRectEmpty' ? 1 : 0),
    )
  }
  // The API may overwrite its return address/arguments, but completion uses the captured frame.
  pair(
    'OffsetRect',
    ['stack', 1, 0xffffffff],
    geometry,
    () => {},
    (p, error) => {
      geometryResult(1)(p, error)
      assert.deepEqual(readRect(p, p.stack), [0, p.stack - 1, 2, 0xfffffffe])
      assert.equal(p.registers.actual_return.value >>> 0, 0xffffffff)
      assert.equal(p.registers.esp.value >>> 0, p.stack + 16)
    },
  )
  for (const pointer of [0xfffffff8, (p) => p.pointer + 56])
    pair(
      'OffsetRect',
      [pointer, -1, 2],
      geometry,
      () => {},
      (p, error) => {
        assert.match(error?.message ?? '', /user32\.dll!OffsetRect:.*caller 0xffffffff/)
        assert.equal(
          p.registers.esp.value >>> 0,
          p.stack,
          'failed call leaves guest frame untouched',
        )
      },
    )
  pair(
    'OffsetRect',
    [0x8000, 1, 1],
    geometry,
    (p) => {
      p.owner.map(0x8000, new Uint8Array(16), false)
    },
    (p, error) => {
      assert.match(error?.message ?? '', /protected/)
      assert.equal(p.registers.esp.value >>> 0, p.stack)
      assert.deepEqual([...p.owner.read(0x8000, 16)], Array(16).fill(0))
    },
  )
  // Rectangle services still select the owner thread and preserve its last-error value.
  const lastErrorId = (p, name) => JSON.parse(p.owner.resolve('kernel32.dll', name)).id
  pair(
    'OffsetRect',
    ['pointer', 1, 1],
    geometry,
    (p) => {
      seedRects(p)
      for (const [thread, value] of [
        [7, 0xabcdef01],
        [9, 0x12345678],
      ]) {
        p.owner.setThread(thread)
        p.owner.call(lastErrorId(p, 'SetLastError'), Uint32Array.of(value))
      }
    },
    (p, error) => {
      geometryResult(1, [1, 1, 11, 11])(p, error)
      assert.equal(p.owner.call(lastErrorId(p, 'GetLastError'), new Uint32Array()), 0xabcdef01)
      p.owner.setThread(9)
      assert.equal(p.owner.call(lastErrorId(p, 'GetLastError'), new Uint32Array()), 0x12345678)
    },
  )
  for (const invalidStack of [false, true]) {
    const before = scenario(false, 'OffsetRect', geometry),
      after = scenario(true, 'OffsetRect', geometry)
    const reason = new Error('geometry cancelled before mutation')
    try {
      for (const p of [before, after]) {
        seedRects(p)
        p.prepare([p.pointer, 100, 100])
        if (invalidStack) p.registers.esp.value = p.memory.buffer.byteLength - 2
        const state = p.snapshot()
        p.cancellation.abort(reason)
        assert.throws(p.run, (error) => error === reason)
        assert.deepEqual(p.snapshot(), state)
        assert.deepEqual(readRect(p, p.pointer), [0, 0, 10, 10])
        assert.equal(p.host.calls, 1000)
      }
      passed++
    } finally {
      before.close()
      after.close()
    }
  }
  for (const count of [0, 39, 40, 99999])
    pair('GetLastError', [], {}, (p) => {
      p.host.calls = count
    })
  pair('SetLastError', [0xffffffff], { reportError: true }, (p) => {
    p.host.calls = 39
  })
  for (const invalidStack of [true, false])
    for (const cancelled of [true, false]) {
      const before = scenario(false),
        after = scenario(true),
        reason = new Error('cancelled precisely')
      try {
        for (const p of [before, after]) {
          p.prepare()
          if (invalidStack) p.registers.esp.value = p.memory.buffer.byteLength - 2
          if (cancelled) p.cancellation.abort(reason)
        }
        assert.deepEqual(outcome(after.run), outcome(before.run))
        assert.deepEqual(after.snapshot(), before.snapshot())
        assert.equal(after.host.calls, before.host.calls)
        assert.equal(after.host.lastCall, before.host.lastCall)
        passed++
      } finally {
        before.close()
        after.close()
      }
    }
  const previousNow = Object.getOwnPropertyDescriptor(performance, 'now')
  const previousDate = Date.now
  try {
    Object.defineProperty(performance, 'now', { configurable: true, value: () => 123.5 })
    Date.now = () => 1700000000000
    pair('GetTickCount', [])
    pair('GetSystemTimeAsFileTime', ['pointer'])
    pair('GetSystemTimeAsFileTime', ['stack'])
  } finally {
    if (previousNow) Object.defineProperty(performance, 'now', previousNow)
    else delete performance.now
    Date.now = previousDate
  }
  for (const api of [
    { schedule: true },
    { wake: true },
    { storage: true },
    { media: true },
    { callback: true },
    { exception: true },
    { variadic: true },
    { arity: 5 },
    { dll: 'user32.dll' },
  ]) {
    const p = scenario(true, 'GetLastError', { api })
    try {
      assert.equal(p.installed, 0)
      assert.equal(p.table.get(16), p.previous)
      passed++
    } finally {
      p.close()
    }
  }
  for (const api of [
    { schedule: true },
    { wake: true },
    { storage: true },
    { media: true },
    { callback: true },
    { exception: true },
    { variadic: true },
    { arity: 5 },
    { stackArgs: 1 },
    { dll: 'gdi32.dll' },
  ]) {
    const p = scenario(true, 'OffsetRect', { ...geometry, api })
    try {
      assert.equal(p.installed, 0)
      assert.equal(p.table.get(16), p.previous)
      passed++
    } finally {
      p.close()
    }
  }
  for (const [name, label] of [
    ['SetRect'],
    ['GetClientRect'],
    ['OffsetRect', 'user32.dll!offsetrect'],
    ['OffsetRect', 'user32.dll!UnknownRect'],
  ]) {
    const p = scenario(true, name, { ...geometry, label })
    try {
      assert.equal(p.installed, 0, name + ' remains on existing path')
      assert.equal(p.table.get(16), p.previous)
      passed++
    } finally {
      p.close()
    }
  }
  // A normal wrapper stays usable after memory growth; status errors must not reuse stale report state.
  const growth = scenario(true, 'VirtualAlloc')
  try {
    growth.prepare([0x30000, 0x20000, 0x3000, 4])
    growth.run()
    growth.prepare([0, 4096, 0x3000, 4])
    growth.run()
    assert.ok(growth.registers.eax.value)
    passed++
  } finally {
    growth.close()
  }
  // Shared process, independent instance globals and closures: selection is never cached.
  const one = scenario(true),
    two = scenario(true, 'GetLastError', { owner: one.owner, memory: one.memory })
  try {
    two.host.bindings = one.imports.bindings
    installNativeImports(two.table, one.imports, two.host) // Reuse one module with independent instance state.
    const setError = JSON.parse(one.owner.resolve('kernel32.dll', 'SetLastError')).id
    for (const [thread, value] of [
      [1, 0x11111111],
      [2, 0xeeeeeeee],
    ]) {
      one.owner.setThread(thread)
      one.owner.call(setError, Uint32Array.of(value))
    }
    one.host.threadId = 1
    two.host.threadId = 2
    for (let i = 0; i < 8; i++) {
      one.prepare()
      two.prepare()
      one.run()
      const saved = one.snapshot()
      two.run()
      assert.deepEqual(one.snapshot(), saved)
      assert.equal(one.registers.eax.value >>> 0, 0x11111111)
      assert.equal(two.registers.eax.value >>> 0, 0xeeeeeeee)
    }
    passed++
  } finally {
    two.close()
    one.close()
  }
  // Signed LEB slot constants beyond127, untouched absent/async imports and atomic bounds rejection.
  for (const offset of [48, 112, 113, 255]) {
    const p = scenario(false)
    let installed
    try {
      const binding = p.imports.bindings[0],
        bindings = Array(offset + 1).fill(null)
      bindings[offset] = binding
      const imports = { bindings, trap: 0 }
      p.host.bindings = bindings
      installSynchronousImports(p.table, imports, (slot) => p.host.dispatchSync(slot))
      installed = installNativeImports(p.table, imports, p.host)
      assert.equal(installed, 1)
      p.prepare()
      p.table.get(offset + 16)()
      assert.equal(p.registers.actual_return.value >>> 0, 0xffffffff)
      assert.equal(p.table.get(offset + 15), null)
      const original = p.table.get(offset + 16)
      assert.throws(
        () => installNativeImports(p.table, { ...imports, trap: 512 }, p.host),
        /function table/,
      )
      assert.equal(p.table.get(offset + 16), original)
      passed++
    } finally {
      p.close()
    }
  }
  const reporting = scenario(true, 'GetLastError', { reportError: true })
  try {
    reporting.host.calls = 39
    reporting.prepare()
    const originalReport = reporting.host.report
    const reportError = new Error('report failure')
    reporting.host.report = () => {
      throw reportError
    }
    assert.throws(reporting.run, (error) => {
      assert.match(error.message, /report failure/)
      assert.equal(error.cause, reportError)
      return true
    })
    reporting.host.report = originalReport
    reporting.prepare()
    reporting.run()
    assert.equal(reporting.host.calls, 41)
    passed++
  } finally {
    reporting.close()
  }

  // The ordinary dispatcher's control-flow sentinel must preserve identity.
  const exiting = scenario(true)
  try {
    const exit = new GuestThreadExit(23)
    exiting.owner.callSmallForThreadStatus = () => {
      throw exit
    }
    exiting.prepare()
    const before = exiting.snapshot()
    assert.throws(exiting.run, (error) => error === exit)
    assert.deepEqual(exiting.snapshot(), before)
    assert.equal(exiting.host.calls, 1000)
    passed++
  } finally {
    exiting.close()
  }

  // Old runtimes and partial feature availability must keep their existing wrappers.
  for (const missing of ['callSmallForThreadStatus', 'takeSyncCallError']) {
    const p = scenario(false)
    try {
      p.owner[missing] = undefined
      assert.equal(installNativeImports(p.table, p.imports, p.host), 0)
      assert.equal(p.table.get(16), p.previous)
      passed++
    } finally {
      p.close()
    }
  }

  // Repeated installs share only the compiled module and one abort global per signal.
  const lifecycle = scenario(false)
  const signal = lifecycle.cancellation.signal
  const add = signal.addEventListener.bind(signal)
  let listeners = 0
  signal.addEventListener = (...args) => {
    if (args[0] === 'abort') listeners++
    return add(...args)
  }
  assert.equal(installNativeImports(lifecycle.table, lifecycle.imports, lifecycle.host), 1)
  const retained = lifecycle.table.get(16)
  assert.equal(installNativeImports(lifecycle.table, lifecycle.imports, lifecycle.host), 1)
  assert.equal(listeners, 1, 'one shared abort listener per host signal, not per installation')
  lifecycle.close()
  assert.throws(
    retained,
    /Test execution closed/,
    'closed wrappers abort before reaching a freed process',
  )
  passed++

  // Abort listeners may synchronously reenter a wrapper before a later mirror runs.
  for (const owned of [false, true])
    for (const invalidStack of [false, true]) {
      const p = scenario(false, 'InterlockedIncrement')
      const threads = owned ? new ThreadHost(p.owner, p.memory) : null
      const cancellation = threads?.cancellation ?? new AbortController()
      let mirrored = false,
        caught
      const reason = new Error('abort listener reentry')
      p.host.threads = threads ?? { signal: cancellation.signal }
      p.host.checkAbort = threads
        ? threads.checkAbort.bind(threads)
        : () => {
            if (mirrored) cancellation.signal.throwIfAborted()
          }
      cancellation.signal.addEventListener('abort', () => {
        try {
          p.run()
        } catch (error) {
          caught = error
        }
      })
      cancellation.signal.addEventListener('abort', () => {
        mirrored = true
      })
      try {
        installNativeImports(p.table, p.imports, p.host)
        p.prepare([p.pointer])
        if (invalidStack) p.registers.esp.value = p.memory.buffer.byteLength - 2
        const before = p.snapshot()
        cancellation.abort(reason)
        assert.equal(caught, reason, 'abort reason wins before stack failure or kernel mutation')
        assert.deepEqual(p.snapshot(), before)
        assert.equal(p.host.calls, 1000)
        assert.equal(p.host.lastCall, undefined)
        assert.deepEqual([...p.owner.read(p.pointer, 4)], [0, 0, 0, 0])
        if (threads) assert.equal(threads.abortGlobal.value, 1)
        passed++
      } finally {
        cancellation.abort()
        await threads?.close()
        p.close()
      }
    }

  function boundary(memory, address, arity) {
    const api = { dll: 'kernel32.dll', id: 1, arity, stackArgs: arity }
    const binding = { label: 'kernel32.dll!Boundary', api }
    const imports = { trap: 0, bindings: [binding] }
    const outcomes = []
    for (const native of [false, true]) {
      let calls = 0
      const owner = {
        setThread() {},
        callSmall() {},
        callSmallForThread() {},
        takeSyncCallError() {},
        callSmallForThreadStatus() {
          calls++
          return 0xffffffff
        },
      }
      const registers = Object.fromEntries(
        ['esp', 'eax', 'actual_return'].map((name) => [
          name,
          new WebAssembly.Global(
            { value: 'i32', mutable: true },
            name === 'esp' ? address : 0x1234,
          ),
        ]),
      )
      const host = new ApiDispatcher(
        { process: owner, memory, registers },
        { yields: 0 },
        () => {},
        () => {},
      )
      const cancellation = new AbortController()
      host.threads = { signal: cancellation.signal }
      host.threadId = 1
      host.calls = 1000
      host.bindings = imports.bindings
      const table = new WebAssembly.Table({ initial: 32, element: 'anyfunc' })
      installSynchronousImports(
        table,
        imports,
        (slot) => host.dispatchSync(slot),
        native ? host : undefined,
      )
      const error = outcome(() => table.get(16)())
      outcomes.push({
        error,
        calls,
        registers: Object.fromEntries(
          Object.entries(registers).map(([name, value]) => [name, value.value >>> 0]),
        ),
      })
      cancellation.abort()
    }
    assert.deepEqual(outcomes[1], outcomes[0], `stack ${address.toString(16)} arity ${arity}`)
    passed++
    return outcomes[1]
  }
  const full = new WebAssembly.Memory({ initial: 65536, maximum: 65536 })
  new DataView(full.buffer).setUint32(0xfffffffc, 0xfedcba98, true)
  const edge = boundary(full, 0xfffffffc, 0)
  assert.equal(edge.error, null)
  assert.equal(edge.registers.eax, 0xffffffff)
  assert.equal(edge.registers.actual_return, 0xfedcba98)
  assert.equal(edge.registers.esp, 0, 'stdcall return wraps ESP exactly at 4GiB')
  assert.equal(
    boundary(full, 0xfffffffc, 1).calls,
    0,
    'argument crossing 4GiB is rejected before the kernel',
  )
  assert.equal(boundary(full, 0xffffffff, 0).calls, 0, 'return-address overflow is rejected')
  const unaligned = new WebAssembly.Memory({ initial: 1, maximum: 2 })
  new DataView(unaligned.buffer).setUint32(1, 0xfedcba98, true)
  assert.equal(boundary(unaligned, 1, 4).error, null, 'unaligned guest stacks remain valid')
  const noMaximum = new WebAssembly.Memory({ initial: 1 })
  assert.equal(boundary(noMaximum, 0, 4).error, null, 'ordinary memories may omit a maximum')
  const shared = new WebAssembly.Memory({ initial: 1, maximum: 2, shared: true })
  new DataView(shared.buffer).setUint32(1, 0xfedcba98, true)
  assert.equal(boundary(shared, 1, 4).error, null, 'shared memories use their matching import type')
  const sharedConstructor = Object.getOwnPropertyDescriptor(globalThis, 'SharedArrayBuffer')
  try {
    Object.defineProperty(globalThis, 'SharedArrayBuffer', { configurable: true, value: undefined })
    assert.equal(
      boundary(shared, 1, 4).error,
      null,
      'shared branding survives a hidden constructor',
    )
  } finally {
    Object.defineProperty(globalThis, 'SharedArrayBuffer', sharedConstructor)
  }
  const crossRealm = runInNewContext(
    'new WebAssembly.Memory({ initial: 1, maximum: 2, shared: true })',
  )
  assert.equal(boundary(crossRealm, 0, 4).error, null, 'shared branding works across realms')
  console.log(`Native import trampolines: ${passed} state/error/ABI/abort/bounds cases passed`)
}
