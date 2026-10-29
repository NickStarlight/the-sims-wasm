import assert from 'node:assert/strict'

export async function pwaSmoke(directory) {
  let serial = 0
  async function check(setup, body) {
    const saved = new Map(
      ['document', 'navigator', 'setTimeout', 'clearTimeout'].map((name) => [
        name,
        Object.getOwnPropertyDescriptor(globalThis, name),
      ]),
    )
    const timers = new Map()
    const status = { textContent: '' }
    const notice = { hidden: true, textContent: '' }
    const registration = Object.assign(new EventTarget(), {
      active: {},
      waiting: null,
      installing: null,
      updates: 0,
      async update() {
        this.updates++
      },
      unregister() {
        assert.fail('checking for updates must preserve other game windows')
      },
    })
    const state = {
      registration,
      status,
      notice,
      timers,
      register: async () => registration,
      worker() {
        return Object.assign(new EventTarget(), { state: 'installing' })
      },
      expire() {
        assert.equal(timers.size, 1)
        const [{ callback, delay }] = timers.values()
        assert.equal(delay, 2500)
        callback()
      },
    }
    setup(state)
    try {
      Object.defineProperty(globalThis, 'document', {
        configurable: true,
        value: {
          querySelector: (selector) => (selector === '#offline-status' ? status : notice),
        },
      })
      Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        value: state.unsupported
          ? {}
          : {
              serviceWorker: {
                register(url, options) {
                  assert.equal(new URL(url).pathname.endsWith('/sw.js'), true)
                  assert.deepEqual(options, { updateViaCache: 'none' })
                  return state.register()
                },
              },
            },
      })
      globalThis.setTimeout = (callback, delay) => {
        const handle = {}
        timers.set(handle, { callback, delay })
        return handle
      }
      globalThis.clearTimeout = (handle) => timers.delete(handle)
      const url = new URL('./pwa.js', directory)
      url.searchParams.set('case', ++serial)
      const { checkForAppUpdate } = await import(url)
      await body(state, checkForAppUpdate)
      assert.equal(timers.size, 0, 'checks clear their deadline timers')
    } finally {
      for (const [name, descriptor] of saved) {
        if (descriptor) Object.defineProperty(globalThis, name, descriptor)
        else delete globalThis[name]
      }
    }
  }

  await check(
    () => {},
    async (state, ready) => {
      assert.equal(await ready(), true)
      assert.equal(state.registration.updates, 1)
      assert.equal(state.notice.hidden, true)
    },
  )
  await check(
    ({ registration, worker }) => {
      registration.active = null
      registration.installing = worker()
    },
    async (state, ready) => {
      assert.equal(await ready(), true, 'initial offline setup does not block a fresh app')
      assert.equal(state.notice.hidden, true)
    },
  )
  await check(
    ({ registration }) => {
      registration.waiting = {}
    },
    async (state, ready) => {
      assert.equal(await ready(), false)
      assert.equal(
        state.registration.updates,
        0,
        'an already waiting update needs no network check',
      )
      assert.equal(state.notice.hidden, false)
      assert.match(state.notice.textContent, /Close all game tabs and windows/)
    },
  )
  await check(
    ({ registration, worker }) => {
      registration.update = async () => {
        registration.installing = worker()
        registration.dispatchEvent(new Event('updatefound'))
      }
    },
    async ({ registration, notice }, ready) => {
      assert.equal(await ready(), false, 'an observed upgrade blocks starting the old app')
      assert.equal(notice.hidden, false)
      assert.match(notice.textContent, /Downloading an update/)
      const worker = registration.installing
      registration.installing = null
      registration.waiting = worker
      worker.state = 'installed'
      worker.dispatchEvent(new Event('statechange'))
      assert.match(notice.textContent, /An update is ready/)
      assert.equal(await ready(), false)
      registration.waiting = null
      worker.state = 'redundant'
      worker.dispatchEvent(new Event('statechange'))
      assert.equal(notice.hidden, true, 'a failed upgrade does not leave a stale blocker')
    },
  )
  for (const failure of ['unsupported', 'registration', 'network']) {
    await check(
      (state) => {
        if (failure === 'unsupported') state.unsupported = true
        if (failure === 'registration')
          state.register = async () => {
            throw new Error('registration unavailable')
          }
        if (failure === 'network')
          state.registration.update = async () => {
            throw new Error('offline')
          }
      },
      async ({ notice }, ready) => {
        assert.equal(await ready(), true, `${failure}: cached games remain available`)
        assert.equal(notice.hidden, true)
      },
    )
  }
  for (const hanging of ['registration', 'network', 'installing']) {
    await check(
      (state) => {
        if (hanging === 'registration') state.register = () => new Promise(() => {})
        else state.registration.update = () => new Promise(() => {})
      },
      async ({ expire, registration, worker }, ready) => {
        const result = ready()
        await Promise.resolve()
        if (hanging === 'installing') {
          registration.installing = worker()
          registration.dispatchEvent(new Event('updatefound'))
        }
        expire()
        assert.equal(
          await result,
          hanging !== 'installing',
          'only observed upgrades block on timeout',
        )
      },
    )
  }
  console.log(
    'App updates: visible waiting/installing notices, pre-Play gate and bounded offline checks passed',
  )
}
