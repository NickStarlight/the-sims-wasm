import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const pageSize = 65536;

// Model short transfers, exclusive OPFS handles, corruption and quota errors.
function filesystem() {
  const entries = new Map();
  let active = 0, peak = 0, fault;
  const root = {
    async getDirectoryHandle(name) {
      assert.equal(name, 'sims-web-assets-v1');
      return root;
    },
    async getFileHandle(name) {
      assert.ok(['pages.bin', 'index.bin'].includes(name));
      let entry = entries.get(name);
      if (!entry) entries.set(name, entry = { bytes: new Uint8Array(), open: false });
      return { async createSyncAccessHandle() {
        if (entry.open) throw new DOMException('Busy', 'NoModificationAllowedError');
        entry.open = true; active++; peak = Math.max(peak, active);
        const check = () => assert.ok(entry.open);
        return {
          getSize() { check(); return entry.bytes.length; },
          read(target, { at }) {
            check();
            const n = Math.min(target.length, 1703, Math.max(0, entry.bytes.length-at));
            target.set(entry.bytes.subarray(at,at+n));
            return n;
          },
          write(source, { at }) {
            check();
            const n = Math.min(source.length, fault === name ? 7 : 1031);
            const bytes = new Uint8Array(Math.max(entry.bytes.length,at+n));
            bytes.set(entry.bytes); bytes.set(source.subarray(0,n),at); entry.bytes=bytes;
            if (fault === name) throw new DOMException('Full after partial write', 'QuotaExceededError');
            return n;
          },
          truncate(size) { check(); entry.bytes=entry.bytes.slice(0,size); },
          flush() { check(); },
          close() { if (entry.open) { entry.open=false; active--; } },
        };
      } };
    },
  };
  return { root, entries, get active() { return active; }, get peak() { return peak; },
    set fault(value) { fault=value; } };
}

export async function assetCacheSmoke(directory = new URL('../../../../web/', import.meta.url)) {
  const { openAssetCache } = await import(new URL('./asset-cache.js', directory));
  const old = Object.getOwnPropertyDescriptor(navigator, 'storage');
  const install = fs => Object.defineProperty(navigator, 'storage', {
    configurable: true, value: { getDirectory: async () => fs.root },
  });
  const asset = (path, size, value, lastModified = 1234) => ({
    path, bytes: new Uint8Array(size).fill(value),
    file: new File([new Uint8Array(size).fill(value)], path, { lastModified }),
  });
  let sourceReads = 0;
  const read = (cache, entry, start = 0, length = entry.file.size) => {
    const detached = cache.read;
    return detached(entry.file, entry.path, start, length, (offset,count) => {
      sourceReads++;
      return entry.bytes.slice(offset,offset+count);
    });
  };
  let cache;
  try {
    const fs = filesystem(); install(fs);
    let a = asset('GameData/Objects.far', pageSize*2+19, 31);
    let b = asset('GameData/Fonts/UI.fnt', 97, 54);
    const ignored = ['UserData/Neighborhood.iff','UserData8/Characters/User.iff',
      'GameData/Temp/a.iff','Saves/a.iff','Sims.exe','ijl10.dll','GameData/options.ini']
      .map(path => asset(path,17,92));
    cache = await openAssetCache([a,b,...ignored],'complete',{budget:pageSize*4});
    assert.equal(cache.stats.status,'ready');
    assert.deepEqual(read(cache,a,pageSize-5,20),a.bytes.slice(pageSize-5,pageSize+15));
    assert.deepEqual(read(cache,a,pageSize*2,19),a.bytes.slice(pageSize*2));
    assert.deepEqual(read(cache,b),b.bytes);
    assert.equal(sourceReads,4);
    assert.equal(cache.stats.pages,4);
    for (const entry of ignored) assert.deepEqual(read(cache,entry),entry.bytes);
    assert.equal(cache.stats.pages,4,'save, temporary and executable files bypass persistent storage');
    cache.flush(); await cache.close();
    assert.equal(fs.active,0);
    assert.equal(fs.peak,2,'only one data and one index handle');
    assert.equal(fs.entries.size,2);

    // New File objects and a different enumeration order still reuse the manifest.
    a = asset('GameData/Objects.far',pageSize*2+19,31);
    b = asset('GameData/Fonts/UI.fnt',97,54);
    const before = sourceReads;
    cache = await openAssetCache([b,a,asset('UserData/Neighborhood.iff',33,1,9999)],'complete',{budget:pageSize*4});
    assert.deepEqual(read(cache,a),a.bytes);
    assert.deepEqual(read(cache,b),b.bytes);
    assert.equal(sourceReads,before,'warm launch serves all asset pages without source reads');
    assert.equal(cache.stats.hits,4);
    const busy = await openAssetCache([a,b],'complete',{budget:pageSize*4});
    assert.equal(busy.stats.status,'busy');
    assert.deepEqual(read(busy,b),b.bytes,'another tab falls back rather than waiting');
    await busy.close(); await cache.close();

    // Every normal metadata change invalidates the shared pair; save changes do not.
    const variants = [
      [asset(a.path,a.file.size,22,1235)],
      [asset(a.path,a.file.size+1,23)],
      [asset('GameData/Renamed.far',a.file.size,24)],
      [a,asset('GameData/Added.iff',4,25)],
      [],
    ];
    for (const variant of variants) {
      cache = await openAssetCache([a,b],'complete',{budget:pageSize*4});
      read(cache,b); await cache.close();
      cache = await openAssetCache([...variant,b],'complete',{budget:pageSize*4});
      const prior = sourceReads;
      assert.deepEqual(read(cache,b),b.bytes);
      assert.equal(sourceReads,prior+1,'changed size, timestamp, path, membership invalidates');
      await cache.close();
    }
    cache = await openAssetCache([b],'different-client',{budget:pageSize*4});
    assert.deepEqual(read(cache,b),b.bytes);
    assert.equal(cache.stats.hits,0,'different executable does not reuse pages');
    await cache.close();

    // Capacity remains bounded; hits affect LRU both during a run and after reopen.
    const tiny = filesystem(); install(tiny);
    const c = asset('GameData/Three.far',pageSize*3,61);
    cache = await openAssetCache([c],'complete',{budget:pageSize*2});
    read(cache,c,0,1); read(cache,c,pageSize,1); read(cache,c,0,1); read(cache,c,pageSize*2,1);
    assert.equal(cache.stats.evictions,1);
    assert.ok(tiny.entries.get('pages.bin').bytes.length<=pageSize*2);
    await cache.close();
    cache = await openAssetCache([c],'complete',{budget:pageSize*2});
    read(cache,c,0,1); read(cache,c,pageSize*2,1);
    assert.equal(cache.stats.hits,2);
    read(cache,c,pageSize,1);
    assert.equal(cache.stats.misses,1,'least recently used page was evicted');
    await cache.close();

    // Raising the limit retains existing slots and makes the new capacity usable.
    const growing = filesystem(); install(growing);
    const expanded = asset('GameData/Expanded.far',pageSize*4,37);
    cache = await openAssetCache([expanded],'complete',{budget:pageSize*2});
    read(cache,expanded,0,1); read(cache,expanded,pageSize,1);
    await cache.close();
    cache = await openAssetCache([expanded],'complete',{budget:pageSize*4});
    const beforeGrowth = sourceReads;
    assert.equal(cache.stats.pages,2,'larger budget preserves the smaller persisted index');
    assert.deepEqual(read(cache,expanded,0,pageSize*2),expanded.bytes.slice(0,pageSize*2));
    assert.equal(sourceReads,beforeGrowth,'existing pages remain warm after capacity growth');
    read(cache,expanded,pageSize*2,pageSize*2);
    assert.equal(cache.stats.pages,4);
    assert.equal(cache.stats.evictions,0,'additional slots are available without eviction');
    assert.equal(growing.entries.get('pages.bin').bytes.length,pageSize*4);
    await cache.close();
    cache = await openAssetCache([expanded],'complete',{budget:pageSize*4});
    const beforeReopen = sourceReads;
    assert.deepEqual(read(cache,expanded),expanded.bytes);
    assert.equal(sourceReads,beforeReopen,'expanded index survives another launch');
    await cache.close();
    cache = await openAssetCache([expanded],'complete',{budget:pageSize*2});
    assert.equal(cache.stats.pages,0,'shrinking below occupied data discards the old cache');
    assert.deepEqual(read(cache,expanded,0,1),expanded.bytes.slice(0,1));
    assert.equal(cache.stats.misses,1);
    assert.ok(growing.entries.get('pages.bin').bytes.length<=pageSize*2);
    await cache.close();

    // A quota failure after overwriting part of a slot must not poison an old index.
    const torn = filesystem(); install(torn);
    const x = asset('GameData/X.iff',pageSize,5), y = asset('GameData/Y.iff',pageSize,9);
    cache = await openAssetCache([x,y],'complete',{budget:pageSize});
    read(cache,x); await cache.close();
    cache = await openAssetCache([x,y],'complete',{budget:pageSize});
    torn.fault='pages.bin';
    assert.deepEqual(read(cache,y),y.bytes,'cache write failure preserves valid source result');
    assert.equal(cache.stats.status,'failed');
    assert.equal(torn.active,0);
    await cache.close(); torn.fault=undefined;
    cache = await openAssetCache([x,y],'complete',{budget:pageSize});
    assert.deepEqual(read(cache,x),x.bytes,'CRC rejects partially overwritten data');
    assert.equal(cache.stats.corruptions,1);
    await cache.close();
    torn.entries.get('index.bin').bytes[15]^=1;
    cache = await openAssetCache([x,y],'complete',{budget:pageSize});
    assert.deepEqual(read(cache,x),x.bytes);
    assert.equal(cache.stats.hits,0,'corrupt index is rebuilt from source');
    torn.fault='index.bin'; cache.flush();
    assert.equal(cache.stats.status,'failed');
    await cache.close(); torn.fault=undefined;
    cache = await openAssetCache([x,y],'complete',{budget:pageSize});
    assert.deepEqual(read(cache,x),x.bytes,'torn index is never trusted');
    await cache.close();

    // Optional storage failure never masks a real missing/short source read.
    Object.defineProperty(navigator,'storage',{configurable:true,value:{getDirectory:async()=>{throw new Error('denied');}}});
    cache = await openAssetCache([x],'complete');
    assert.equal(cache.stats.status,'unavailable');
    assert.deepEqual(read(cache,x),x.bytes);
    assert.throws(() => cache.read(x.file,x.path,0,1,()=>new Uint8Array()),/Short read/);
    assert.throws(() => cache.read(x.file,x.path,0,1,()=>{throw new Error('source failed');}),/source failed/);
    cache.flush(); await cache.close(); await cache.close();
    install(torn);
    cache=await openAssetCache([x],'complete');
    assert.equal(cache.stats.status,'ready','failed open released its lock');
    await cache.close();
    console.log('Persistent assets: cold/warm reload, exclusions, metadata invalidation, LRU bounds, busy tabs, torn pages/index and fallback passed');
  } finally {
    await cache?.close();
    if(old)Object.defineProperty(navigator,'storage',old);else delete navigator.storage;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href)
  await assetCacheSmoke();
