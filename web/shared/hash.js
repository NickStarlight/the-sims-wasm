export async function hashBytes(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))

  return [...digest].map((byte) => byte.toString(16).padStart(2, '0')).join('')
}
