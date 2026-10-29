import initRuntime, { Process } from './runtime.js'
import { API_BASE } from './linker.js'
import { createFileReaders } from './file-source.js'
import { openAssetCache } from './asset-cache.js'

export const registerNames = [
  'eax',
  'ecx',
  'edx',
  'ebx',
  'esp',
  'ebp',
  'esi',
  'edi',
  'flags',
  'fp_top',
  'fp_status',
  'trap_va',
  'call_target',
  'resume_next',
  'actual_return',
  'teb_base',
]
const TEB = 0x1f000000
const ready = initRuntime()

export async function createMachine(program, report) {
  await ready
  const memory = new WebAssembly.Memory({ initial: 256, maximum: 32768 })
  const process = new Process(memory, 0x1000000, 0x80000000)

  let assetCache

  try {
    const files = program.files ?? []
    report(`Loading ${files.length} game files…`)
    const reader = typeof FileReaderSync === 'function' ? new FileReaderSync() : null
    assetCache = reader ? await openAssetCache(files, program.clientId) : null
    const assetStats = {}
    const fileReader = reader
      ? createFileReaders((blob) => reader.readAsArrayBuffer(blob), undefined, undefined, {
          readPage: assetCache?.read,
          stats: assetStats,
        })
      : null

    for (const { file, path } of files) {
      if (fileReader) process.addFileReader(path, file.size, fileReader(file, path))
      else process.addFile(path, new Uint8Array(await file.arrayBuffer()))
    }

    if (program.initialRegistry) process.importRegistry(JSON.stringify(program.initialRegistry))

    if (program.registry) {
      try {
        process.importRegistry(program.registry)
      } catch {
        report('Stored registry is malformed; using a fresh process registry.')
      }
    }

    mapMemory(process, memory, program)

    return {
      process,
      memory,
      assetCache,
      assetStats,
      ...createThreadContext(program.metadata, { id: 1, teb: TEB, stackBase: 0xeff010 }),
    }
  } catch (error) {
    // A raw Wasm trap can leave a binding borrowed. Cleanup must still release
    // independent resources without replacing the error which caused shutdown.
    try {
      await assetCache?.close()
    } catch {
      /* Preserve the setup failure. */
    }

    try {
      process.free()
    } catch {
      /* Preserve the setup failure, including a poisoned Wasm binding. */
    }

    throw error
  }
}

export function createThreadContext(metadata, { id, teb, stackBase }) {
  const registers = Object.fromEntries(
    registerNames.map((name) => [name, new WebAssembly.Global({ value: 'i32', mutable: true }, 0)]),
  )
  registers.esp.value = stackBase - 16
  registers.fp_top.value = 7
  registers.teb_base.value = teb
  const table = new WebAssembly.Table({ element: 'anyfunc', initial: metadata.trampBase + 16384 })

  return { registers, table, threadId: id }
}

function mapMemory(process, memory, { metadata, plan, imageBytes, commandLine }) {
  const commandLineAddr = metadata.commandLineAddr

  if (
    !Number.isInteger(commandLineAddr) ||
    commandLineAddr < 0x10000 ||
    commandLineAddr > 0xfffff000 ||
    commandLineAddr % 4096 !== 0
  )
    throw new Error('Invalid bundled command-line address. Rebuild the Complete Collection client.')

  const commandLineText = commandLine ?? 'C:\\Sims\\Sims.exe'
  const commandLineBytes = new TextEncoder().encode(`${commandLineText}\0`)

  if (commandLineText.includes('\0') || commandLineBytes.length > 4096)
    throw new Error('The game command line must fit in 4096 bytes and contain no NUL characters.')

  process.reserve(0, 0x10000)
  process.reserve(API_BASE, 0x100000)
  process.adopt(0xe00000, 0x100000, true)
  process.adopt(TEB, 0x10000, true)
  process.adopt(commandLineAddr, 4096, true)
  process.adopt(metadata.tableAddr, (metadata.funcCount + 4) * 4, true)

  for (const image of plan.images) process.adopt(image.base, image.sizeOfImage, true)

  for (const span of plan.spans) {
    new Uint8Array(memory.buffer, span.address, span.length).set(
      imageBytes.subarray(span.offset, span.offset + span.length),
    )
  }

  process.write(commandLineAddr, commandLineBytes)
  process.setCommandLine(commandLineAddr)

  const put = (address, value) => new DataView(memory.buffer).setUint32(address, value, true)
  const threadFields = [
    [0, 0xffffffff],
    [4, 0xf00000],
    [8, 0xe00000],
    [0x18, TEB],
    [0x24, 1],
  ]

  for (const [offset, value] of threadFields) put(TEB + offset, value)

  metadata.funcAddrs.forEach((address, index) => put(metadata.tableAddr + index * 4, address))
}
