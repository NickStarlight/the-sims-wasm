import { hashBytes } from '../shared/hash.js'
import { journalName } from './save-paths.js'

const backupMagic = new TextEncoder().encode('SIMSAVE1')
const backupHeaderSize = 44
const maximumManifestSize = 16 * 1024 * 1024
const verifiedBackups = new WeakSet()
let validatorsReady

async function backupValidators() {
  validatorsReady ??= import('../runtime.js')
    .then(async (runtime) => {
      await runtime.default()

      return runtime
    })
    .catch((error) => {
      validatorsReady = undefined
      throw error
    })

  return validatorsReady
}

export async function inspectProgressBackup(file) {
  const { manifest, offset: start } = await readManifest(file)
  let offset = start

  const runtime = await backupValidators()
  const names = new Set()
  const files = []
  const registry = []

  for (const entry of manifest.registry) {
    if (
      !entry ||
      typeof entry.clientId !== 'string' ||
      !/^[0-9a-f]{64}$/.test(entry.clientId) ||
      typeof entry.snapshot !== 'string' ||
      names.has(entry.clientId)
    )
      throw invalid()

    runtime.validateRegistrySnapshot(entry.snapshot)
    names.add(entry.clientId)
    registry.push(Object.freeze({ clientId: entry.clientId, snapshot: entry.snapshot }))
  }

  for (const entry of manifest.journals) {
    if (
      !entry ||
      typeof entry.name !== 'string' ||
      !journalName.test(entry.name) ||
      names.has(entry.name) ||
      !Number.isSafeInteger(entry.size) ||
      entry.size < 0 ||
      entry.size > file.size - offset ||
      typeof entry.sha256 !== 'string' ||
      !/^[0-9a-f]{64}$/.test(entry.sha256)
    )
      throw invalid()

    const bytes = await file.slice(offset, offset + entry.size).arrayBuffer()

    if ((await hashBytes(bytes)) !== entry.sha256)
      throw new Error('The backup save data is damaged. Current progress has not changed.')

    runtime.validateSaveJournal(new Uint8Array(bytes))
    names.add(entry.name)
    files.push(Object.freeze({ name: entry.name, data: new Blob([bytes]) }))
    offset += entry.size
  }

  if (offset !== file.size) throw invalid()

  const backup = Object.freeze({
    createdAt: manifest.createdAt,
    files: Object.freeze(files),
    registry: Object.freeze(registry),
  })
  verifiedBackups.add(backup)

  return backup
}

export function requireVerifiedBackup(backup) {
  if (!verifiedBackups.has(backup)) throw new Error('Choose and validate a progress backup first.')
}

export async function createProgressBackup(files, journals, registry) {
  const createdAt = new Date().toISOString()
  const manifest = new TextEncoder().encode(
    JSON.stringify({
      format: 'the-sims-wasm-progress',
      version: 1,
      createdAt,
      registry,
      journals,
    }),
  )

  if (manifest.length > maximumManifestSize) throw new Error('The backup metadata is too large.')

  const header = new Uint8Array(backupHeaderSize)
  header.set(backupMagic)
  new DataView(header.buffer).setUint32(8, manifest.length, true)
  header.set(new Uint8Array(await crypto.subtle.digest('SHA-256', manifest)), 12)

  return {
    blob: new Blob([header, manifest, ...files], { type: 'application/octet-stream' }),
    createdAt,
  }
}

function invalid() {
  return new Error('This is not a valid The Sims progress backup.')
}

async function readManifest(file) {
  if (!(file instanceof Blob) || file.size < backupHeaderSize) throw invalid()

  const header = new Uint8Array(await file.slice(0, backupHeaderSize).arrayBuffer())

  if (backupMagic.some((byte, index) => header[index] !== byte)) throw invalid()

  const manifestSize = new DataView(header.buffer).getUint32(8, true)
  const offset = backupHeaderSize + manifestSize

  if (!manifestSize || manifestSize > maximumManifestSize || offset > file.size) throw invalid()

  const manifestBytes = await file.slice(backupHeaderSize, offset).arrayBuffer()
  const manifestHash = new Uint8Array(await crypto.subtle.digest('SHA-256', manifestBytes))

  if (manifestHash.some((byte, index) => header[index + 12] !== byte))
    throw new Error('The backup metadata is damaged. Current progress has not changed.')

  let manifest

  try {
    manifest = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(manifestBytes))
  } catch {
    throw invalid()
  }

  if (
    manifest?.format !== 'the-sims-wasm-progress' ||
    manifest.version !== 1 ||
    typeof manifest.createdAt !== 'string' ||
    !Number.isFinite(Date.parse(manifest.createdAt)) ||
    !Array.isArray(manifest.journals) ||
    !Array.isArray(manifest.registry) ||
    (!manifest.journals.length && !manifest.registry.length)
  )
    throw invalid()

  return { manifest, offset }
}
