export const API_BASE = 0x800000

export function prepareImports(process, plan, metadata, memory) {
  const imports = plan.images.flatMap((image) => image.imports)
  const native = JSON.parse(process.imports())
  const bindings = imports.map((item) => {
    const name = item.name ?? '#' + item.ordinal
    const api = process.resolve(item.dll, name)

    return { label: `${item.dll}!${name}`, api: api ? JSON.parse(api) : null }
  })
  const nativeBindings = native.map((api) => {
    const address = API_BASE + bindings.length * 4
    bindings.push({ label: `${api.dll}!${api.name}`, api })

    return { dll: api.dll, name: api.name, address }
  })

  if (bindings.length + 16 > 16384) throw new Error('Import trampoline capacity exceeded.')

  process.installModules(
    JSON.stringify({
      images: plan.images,
      nativeDlls: [...new Set(native.map((api) => api.dll))],
      nativeBindings,
    }),
  )
  const aliases = bindings.map((_, index) => [
    metadata.funcCount + 1 + index,
    metadata.trampBase + index + 16,
  ])
  const functions = new Map(metadata.funcAddrs.map((address, index) => [address, index + 1]))
  const view = new DataView(memory.buffer)

  for (const [index, item] of imports.entries()) {
    const address =
      process.exportAddress(item.dll, item.name ?? '#' + item.ordinal) || API_BASE + index * 4
    view.setUint32(item.iatVa, address, true)

    if (functions.has(address)) {
      aliases[index][1] = functions.get(address)
    } else if (address >= API_BASE && address < API_BASE + bindings.length * 4) {
      const slot = (address - API_BASE) / 4
      aliases[index][1] = metadata.trampBase + slot + 16
    } else {
      throw new Error(
        `Import ${item.dll}!${item.name ?? item.ordinal} targets an untranslated function 0x${address.toString(16)}.`,
      )
    }
  }

  return { bindings, aliases, trap: metadata.trampBase }
}

// Each instance's functions capture its own register globals. Never share tables between threads.
export function attachImports(table, { aliases, trap }) {
  table.set(0, table.get(trap))

  for (const [destination, source] of aliases) table.set(destination, table.get(source))
}

export function linkImports(process, plan, metadata, memory, table) {
  const imports = prepareImports(process, plan, metadata, memory)
  attachImports(table, imports)

  return imports.bindings
}

export function initializationOrder(images) {
  const modules = new Map(
    images
      .filter((image) => image.isDll)
      .map((image) => [(image.name ?? image.path.split('\\').at(-1)).toLowerCase(), image]),
  )
  const visited = new Set()
  const order = []

  function visit(image) {
    if (visited.has(image.base)) return

    visited.add(image.base)

    for (const dependency of image.imports) {
      const module = modules.get(dependency.dll.toLowerCase())

      if (module) visit(module)
    }

    order.push(image)
  }

  modules.forEach(visit)

  return order
}
