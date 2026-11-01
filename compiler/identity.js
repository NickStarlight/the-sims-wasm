export function validateClientId(clientId) {
  if (typeof clientId !== 'string' || !/^[0-9a-f]{64}$/.test(clientId))
    throw new Error('Invalid game identity. Reload the page.')
}
