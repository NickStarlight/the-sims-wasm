// The packager fills these from the complete, staged build (including the Wasm files).
const precache = ["app/error-report.js","app/folder-selection.js","app/folder.js","app/game-session.js","app/game-view.js","app/launcher.js","app/main.js","app/performance.js","app/progress.js","app/pwa.js","audio/audio-output.js","audio/launcher-music.js","audio/launcher-music.wav","compiler-build.json","compiler.d.ts","compiler.js","compiler/game.js","compiler/identity.js","compiler_bg.wasm","compiler_bg.wasm.d.ts","display/frame-output.js","display/frame-renderer.js","display/keyboard.js","display/presentation.js","display/shaders.js","images/icon-180.png","images/icon-192.png","images/icon-512.png","images/icon.svg","images/launcher-background.png","images/launcher-easter-egg.png","images/launcher-folder-active.png","images/launcher-folder-disabled.png","images/launcher-folder-hover.png","images/launcher-folder.png","images/launcher-info-active.png","images/launcher-info-disabled.png","images/launcher-info-hover.png","images/launcher-info.png","images/launcher-maxis-active.png","images/launcher-maxis-hover.png","images/launcher-maxis.png","images/launcher-play-active.png","images/launcher-play-hover.png","images/launcher-play.png","images/launcher-reset-active.png","images/launcher-reset-hover.png","images/launcher-reset.png","index.html","manifest.webmanifest","runtime.d.ts","runtime.js","runtime/callbacks.js","runtime/conditional-suspension.js","runtime/dispatch.js","runtime/exceptions.js","runtime/execution.js","runtime/file-source.js","runtime/guest-memory.js","runtime/guest.js","runtime/linker.js","runtime/machine.js","runtime/memory-ops.js","runtime/native-imports.js","runtime/runtime-access.js","runtime/scheduler.js","runtime/sync-imports.js","runtime/threads.js","runtime_bg.wasm","runtime_bg.wasm.d.ts","shared/hash.js","storage/asset-cache.js","storage/backup-format.js","storage/compiled-cache.js","storage/index.js","storage/journal.js","storage/progress.js","storage/registry.js","storage/save-paths.js","styles/main.css","workers/runtime-worker.js","workers/worker.js"]
const cachePrefix = `sims-web:${self.registration.scope}:`
const cacheName = `${cachePrefix}d73fd965eafd53b4`
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
