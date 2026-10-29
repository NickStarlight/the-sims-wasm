// The packager fills these from the complete, staged build (including the Wasm files).
const precache = []
const cachePrefix = `sims-web:${self.registration.scope}:`
const cacheName = `${cachePrefix}__BUILD_ID__`
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
