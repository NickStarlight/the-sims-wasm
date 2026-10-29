// The packager fills these from the complete, staged build (including the Wasm files).
const precache = ["asset-cache.js","audio-output.js","callbacks.js","compiled-cache.js","compiler-build.json","compiler.d.ts","compiler.js","compiler_bg.wasm","compiler_bg.wasm.d.ts","conditional-suspension.js","dispatch.js","exceptions.js","execution.js","file-source.js","folder.js","frame-output.js","frame-renderer.js","game.js","guest-memory.js","guest.js","icon-180.png","icon-192.png","icon-512.png","icon.svg","index.html","keyboard.js","launcher-background.png","launcher-folder.png","launcher-info.png","launcher-music.wav","launcher-play.png","linker.js","machine.js","main.js","manifest.webmanifest","memory-ops.js","native-imports.js","presentation.js","pwa.js","runtime-worker.js","runtime.d.ts","runtime.js","runtime_bg.wasm","runtime_bg.wasm.d.ts","scheduler.js","storage.js","style.css","sync-imports.js","threads.js","worker.js"]
const cachePrefix = `sims-web:${self.registration.scope}:`
const cacheName = `${cachePrefix}95967fe61b03741f`
const urls = new Set(precache.map((path) => new URL(path, self.registration.scope).href))

self.addEventListener('install', (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(cacheName)
      await cache.addAll([...urls].map((url) => new Request(url, { cache: 'reload' })))
    })(),
  )
  // Keep updates waiting while any tab is using the previous runtime.
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys()
      await Promise.all(
        names
          .filter((name) => name.startsWith(cachePrefix) && name !== cacheName)
          .map((name) => caches.delete(name)),
      )
    })(),
  )
})

self.addEventListener('fetch', (event) => {
  if (event.request.method !== 'GET') return

  const url = new URL(event.request.url)
  const scope = new URL(self.registration.scope)

  if (url.origin !== scope.origin || !url.pathname.startsWith(scope.pathname)) return

  const key =
    event.request.mode === 'navigate'
      ? new URL('index.html', scope).href
      : `${url.origin}${url.pathname}`

  if (!urls.has(key)) return

  event.respondWith(
    (async () => {
      const cache = await caches.open(cacheName)

      return (await cache.match(key)) ?? fetch(event.request)
    })(),
  )
})
