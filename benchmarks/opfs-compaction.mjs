import { chromium } from 'playwright'
const browser = await chromium.connectOverCDP(process.argv[2] || 'http://127.0.0.1:9229')
const context = await browser.newContext({ serviceWorkers: 'block' })
try {
  const page = await context.newPage()
  await page.goto('http://127.0.0.1:8091')
  console.log(
    await page.evaluate(async () => {
      const code = `import {openSaveStorage} from 'http://127.0.0.1:8091/storage/journal.js';
 onmessage=async()=>{try{
 const root=await navigator.storage.getDirectory();const dir=await root.getDirectoryHandle('sims-web-saves-v1',{create:true});
 const name='c'.repeat(64)+'-e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.journal';
 const entry=await dir.getFileHandle(name,{create:true});const access=await entry.createSyncAccessHandle();
 access.write(new Uint8Array(16*1024*1024));access.flush();access.close();
 let size=0;const process={restoreJournal(bytes){size=bytes.length;return size},compactJournal(){return new Uint8Array([1,2,3])},takeFileChanges(){return new Uint8Array()}};
 let storage=await openSaveStorage([],process,'c'.repeat(64));await storage.close();
 if((await entry.getFile()).size!==3)throw Error('not compacted');
 storage=await openSaveStorage([],process,'c'.repeat(64));await storage.close();
 if(size!==3)throw Error('reload failed');postMessage('Real Chrome OPFS transactional compaction and reopen passed');
 }catch(e){postMessage({error:String(e),stack:e.stack})}}`
      const url = URL.createObjectURL(new Blob([code], { type: 'text/javascript' }))
      const worker = new Worker(url, { type: 'module' })
      return await new Promise((resolve, reject) => {
        worker.onmessage = (e) => {
          worker.terminate()
          URL.revokeObjectURL(url)
          e.data.error ? reject(Error(e.data.error)) : resolve(e.data)
        }
        worker.onerror = reject
        worker.postMessage(null)
      })
    }),
  )
} finally {
  await context.close()
  await browser.close()
}
