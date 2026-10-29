const status = document.querySelector('#offline-status')

if ('serviceWorker' in navigator) {
  status.textContent = 'Preparing offline play…'
  navigator.serviceWorker
    .register(new URL('./sw.js', import.meta.url), { updateViaCache: 'none' })
    .then((registration) => {
      function updateStatus() {
        status.textContent = registration.waiting
          ? 'An update is ready. Close all game windows and reopen to use it.'
          : registration.active
            ? 'Ready for offline play. Access to your game folder is still required.'
            : 'Downloading files for offline play…'
      }

      function watch(worker) {
        worker?.addEventListener('statechange', () => {
          if (worker.state === 'redundant') {
            status.textContent =
              'Offline setup failed. Reopen online to retry; online play is still available.'
          } else updateStatus()
        })
      }

      updateStatus()
      watch(registration.installing)
      registration.addEventListener('updatefound', () => watch(registration.installing))
    })
    .catch(() => {
      status.textContent = 'Offline setup is unavailable. You can still play while online.'
    })
}
