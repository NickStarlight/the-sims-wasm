import { validateClientId } from '../compiler/identity.js'
import { registryPrefix } from './save-paths.js'

export function loadRegistry(clientId) {
  validateClientId(clientId)

  try {
    return localStorage.getItem(`${registryPrefix()}${clientId}`)
  } catch {
    return null
  }
}

export function storeRegistry(snapshot, clientId) {
  validateClientId(clientId)

  try {
    localStorage.setItem(`${registryPrefix()}${clientId}`, snapshot)
  } catch {
    /* Registry remains usable when storage is unavailable. */
  }
}
