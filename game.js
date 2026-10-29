import { openCompiledCache } from './compiled-cache.js'

const commandLine = 'C:\\Sims\\Sims.exe -skip_intro'

export function validateClientId(clientId) {
  if (typeof clientId !== 'string' || !/^[0-9a-f]{64}$/.test(clientId))
    throw new Error('Invalid game identity. Reload the page.')
}

function pathKey(path) {
  return path.replaceAll('\\', '/').toLowerCase()
}

async function digest(bytes) {
  const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))

  return [...hash].map((byte) => byte.toString(16).padStart(2, '0')).join('')
}

async function compilerBuild() {
  const response = await fetch('./compiler-build.json')

  if (!response.ok) throw new Error('The compiler could not be loaded. Reload and try again.')

  const build = await response.json()
  const profile = build.profile

  if (
    build.version !== 1 ||
    !profile ||
    !Array.isArray(profile.modules) ||
    !Array.isArray(profile.requiredFiles)
  )
    throw new Error('Unsupported compiler configuration. Reload the page.')

  validateClientId(build.compilerId)
  validateClientId(build.wasmSha256)
  validateClientId(profile.sha256)

  return build
}

function programFrom(compiled, module, files) {
  return {
    files,
    module,
    metadata: compiled.metadata,
    plan: compiled.plan,
    imageBytes: compiled.imageBytes,
    clientId: compiled.clientId,
    commandLine: compiled.commandLine,
    initialRegistry: compiled.initialRegistry,
  }
}

async function compileGame(build, binaries) {
  const [javascript, wasm] = await Promise.all(
    ['compiler.js', 'compiler_bg.wasm'].map(async (name) => {
      const response = await fetch(`./${name}`, { cache: 'reload' })

      if (!response.ok) throw new Error('The compiler could not be loaded. Reload and try again.')

      return new Uint8Array(await response.arrayBuffer())
    }),
  )
  const compilerBytes = new Uint8Array(javascript.length + wasm.length)
  compilerBytes.set(javascript)
  compilerBytes.set(wasm, javascript.length)

  if (
    (await digest(wasm)) !== build.wasmSha256 ||
    (await digest(compilerBytes)) !== build.compilerId
  )
    throw new Error('Compiler files do not match. Close all game tabs and reopen the page.')

  // Import the verified bytes, rather than making a second URL request that
  // could race a deployment or reuse a different HTTP/module-cache response.
  const moduleUrl = URL.createObjectURL(new Blob([javascript], { type: 'text/javascript' }))
  let bindings

  try {
    bindings = await import(moduleUrl)
  } finally {
    URL.revokeObjectURL(moduleUrl)
  }

  await bindings.default({ module_or_path: wasm })
  const profile = JSON.parse(bindings.completeProfileJson())

  if (
    profile.sha256 !== build.profile.sha256 ||
    profile.modules.length !== build.profile.modules.length ||
    profile.modules.some(
      ({ name, base }, index) =>
        name !== build.profile.modules[index].name || base !== build.profile.modules[index].base,
    ) ||
    JSON.stringify(profile.requiredFiles) !== JSON.stringify(build.profile.requiredFiles)
  )
    throw new Error('Compiler files do not match. Close all game tabs and reopen the page.')

  let compiler
  let artifact

  try {
    compiler = new bindings.Compiler(binaries[0].bytes)

    for (let index = 0; index < profile.modules.length; index++)
      compiler.addModule(binaries[index + 1].bytes, profile.modules[index].base)

    artifact = compiler.compile(Uint32Array.from(profile.roots))
    const plan = JSON.parse(compiler.loadPlanJson())

    plan.images.forEach((image, index) => {
      image.path = `C:\\Sims\\${binaries[index].path}`
    })

    return {
      version: 1,
      wasm: artifact.wasm(),
      metadata: JSON.parse(artifact.runtimeMetadataJson()),
      plan,
      imageBytes: compiler.loadBytes(),
      clientId: profile.sha256,
      commandLine,
      initialRegistry: profile.initialRegistry,
    }
  } finally {
    artifact?.free()
    compiler?.free()
  }
}

export async function loadGame(files, onPhase = () => {}, onProgress = () => {}) {
  const selected = new Map(files.map((entry) => [pathKey(entry.path), entry.file]))

  for (const path of ['GameData/Objects/Objects.far', 'UserData/Neighborhood.iff', 'Sims.exe']) {
    if (!selected.has(pathKey(path)))
      throw new Error(`Choose your Complete Collection installation, including ${path}.`)
  }

  if (!globalThis.crypto?.subtle)
    throw new Error('Open this site over HTTPS or localhost to prepare your game locally.')

  const mark = (name) => onPhase({ name, atMs: performance.timeOrigin + performance.now() })
  mark('local-files-check-start')
  onProgress('Checking your game files…')
  const build = await compilerBuild()

  for (const path of build.profile.requiredFiles) {
    if (!selected.has(pathKey(path)))
      throw new Error(`Complete Collection requires ${path}. Choose its complete installation.`)
  }

  const names = ['Sims.exe', ...build.profile.modules.map(({ name }) => name)]

  for (const name of names) {
    if (!selected.has(pathKey(name)))
      throw new Error(`Your Complete Collection installation is missing ${name}.`)
  }

  const binaries = await Promise.all(
    names.map(async (path) => {
      const bytes = new Uint8Array(await selected.get(pathKey(path)).arrayBuffer())

      return { path, bytes, sha256: await digest(bytes) }
    }),
  )

  if (binaries[0].sha256 !== build.profile.sha256)
    throw new Error('This Sims.exe is not the supported Complete Collection v1.91 executable.')

  const key = JSON.stringify({
    version: 1,
    compilerId: build.compilerId,
    inputs: binaries.map(({ path, sha256 }) => ({ path, sha256 })),
    commandLine,
  })
  const cache = await openCompiledCache()

  try {
    const prepare = async () => {
      const cached = await cache.get(key)

      if (cached && cached.clientId === binaries[0].sha256 && cached.commandLine === commandLine) {
        try {
          onProgress('Loading your cached game…')
          const module = await WebAssembly.compile(cached.wasm)
          mark('local-compile-cache-hit')
          mark('client-ready')

          return programFrom(cached, module, files)
        } catch {
          await cache.delete(key)
        }
      }

      mark('local-compile-cache-miss')
      onProgress('Preparing your game locally. The first launch takes longer…')
      mark('local-compile-start')
      const compiled = await compileGame(build, binaries)
      mark('local-compile-done')
      onProgress('Starting your game…')
      const module = await WebAssembly.compile(compiled.wasm)
      await cache.put(key, compiled)
      mark('client-ready')

      return programFrom(compiled, module, files)
    }

    return typeof navigator !== 'undefined' && navigator.locks?.request
      ? await navigator.locks.request('sims-web:compilation', prepare)
      : await prepare()
  } finally {
    cache.close()
  }
}
