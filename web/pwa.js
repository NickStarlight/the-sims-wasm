const status = document.querySelector('#offline-status')
const updateNotice = document.querySelector('#update-status')
let currentRegistration
let registrationReady = Promise.resolve(null)

function updateBlocksPlay(registration) {
  return Boolean(registration?.waiting || (registration?.active && registration.installing))
}

function updateStatus(registration) {
  status.textContent = registration.waiting
    ? 'An update is ready. Close all game windows and reopen to use it.'
    : registration.active
      ? 'Ready for offline play. Access to your game folder is still required.'
      : 'Downloading files for offline play…'
  updateNotice.hidden = !updateBlocksPlay(registration)
  updateNotice.textContent = registration.waiting
    ? 'An update is ready. Close all game tabs and windows, then reopen this site to play.'
    : registration.active && registration.installing
      ? 'Downloading an update. Wait for it to finish before starting the game.'
      : ''
}

// Keep updates waiting until old game windows close. Starting a game while an
// upgrade is already waiting would keep that old version alive indefinitely.
export async function checkForAppUpdate() {
  if (updateBlocksPlay(currentRegistration)) {
    updateStatus(currentRegistration)

    return false
  }

  let timer
  const checking = registrationReady.then(async (registration) => {
    if (!registration) return true

    if (updateBlocksPlay(registration)) {
      updateStatus(registration)

      return false
    }

    try {
      await registration.update()
    } catch {
      // Existing cached games remain available when the network is unavailable.
    }

    updateStatus(registration)

    return !updateBlocksPlay(registration)
  })

  try {
    return await Promise.race([
      checking,
      new Promise((resolve) => {
        timer = setTimeout(() => resolve(!updateBlocksPlay(currentRegistration)), 2500)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

if ('serviceWorker' in navigator) {
  status.textContent = 'Preparing offline play…'
  registrationReady = navigator.serviceWorker
    .register(new URL('./sw.js', import.meta.url), { updateViaCache: 'none' })
    .then((registration) => {
      currentRegistration = registration

      function watch(worker) {
        worker?.addEventListener('statechange', () => {
          updateStatus(registration)

          if (worker.state === 'redundant') {
            status.textContent =
              'Offline setup failed. Reopen online to retry; online play is still available.'
          }
        })
      }

      updateStatus(registration)
      watch(registration.installing)
      registration.addEventListener('updatefound', () => {
        updateStatus(registration)
        watch(registration.installing)
      })

      return registration
    })
    .catch(() => {
      status.textContent = 'Offline setup is unavailable. You can still play while online.'

      return null
    })
}
