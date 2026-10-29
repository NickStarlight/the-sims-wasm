// Original audio-output smoke contracts exercised through the actual Wasm owner and worklet source.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

export async function audioSmoke(Process) {
  const owner = new Process(new WebAssembly.Memory({ initial: 4, maximum: 128 }), 0x10000, 0x800000);
  try {
    const imports = JSON.parse(owner.imports()).filter(api => api.dll === 'dsound.dll');
    owner.installModules(JSON.stringify({ images: [], nativeDlls: ['dsound.dll'], nativeBindings: imports.map((api,i) => ({ dll: api.dll, name: api.name, address: 0x800000+i*4 })) }));
    const invoke = (name,args) => { const api=imports.find(api => api.name===name); return owner.call(api.id, Uint32Array.from(args), api.clock ? 0 : undefined); };
    const write = (p,n) => owner.write(p,new Uint8Array(Uint32Array.of(n).buffer));
    const read = p => new DataView(owner.read(p,4).buffer).getUint32(0,true);
    const out=owner.heapAlloc(4),desc=owner.heapAlloc(20),fmt=owner.heapAlloc(18),lock=owner.heapAlloc(16);
    invoke('DirectSoundCreate',[0,out,0]);const device=read(out);
    owner.write(fmt,Uint8Array.of(1,0,2,0,0x80,0xbb,0,0,0,0xee,2,0,4,0,16,0,0,0));
    for(const [offset,value] of [[0,20],[4,0],[8,16],[16,fmt]]) write(desc+offset,value);
    assert.equal(invoke('DS_Device_3',[device,desc,out,0]),0); const buffer=read(out);
    invoke('DS_Buffer_11',[buffer,0,16,lock,lock+4,lock+8,lock+12,0]);const data=read(lock);
    owner.write(data,Uint8Array.from(Array.from({length:4},()=>[0,32,0,192]).flat()));
    invoke('DS_Buffer_19',[buffer,data,16,0,0]); invoke('DS_Buffer_12',[buffer,0,0,1]);
    const block=owner.mixAudio(256,48000,0),left=block.left(),right=block.right();block.free();
    assert.deepEqual([...left],Array(256).fill(.25));assert.deepEqual([...right],Array(256).fill(-.5));
    // Streaming refills replace addressed frames of the playing circular buffer.
    invoke('DS_Buffer_11',[buffer,4,4,lock,lock+4,lock+8,lock+12,0]);const refill=read(lock);
    owner.write(refill,Uint8Array.of(0,64,0,192));invoke('DS_Buffer_19',[buffer,refill,4,0,0]);
    invoke('DS_Buffer_13',[buffer,0]);
    const streamed=owner.mixAudio(8,48000,0);
    assert.deepEqual([...streamed.left()],[.25,.5,.25,.25,.25,.5,.25,.25]);streamed.free();
    invoke('DS_Buffer_18',[buffer]);const silence=owner.mixAudio(256,48000,0);assert.ok(silence.left().every(v=>v===0));silence.free();
  } finally { owner.free(); }

  const source=readFileSync(new URL('../../../../web/audio-output.js',import.meta.url),'utf8');
  const {serveAudio,AUDIO_PROCESSOR_SOURCE}=await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
  const channel=new MessageChannel();
  const close=serveAudio(channel.port1,()=> (left,right)=>{left.fill(.25);right.fill(-.5);});
  const reply=new Promise(resolve=>{channel.port2.onmessage=({data})=>resolve(data);});
  channel.port2.postMessage({frames:256,rate:48000});const pcm=await reply;
  assert.ok(pcm.left.every(v=>v===.25));assert.ok(pcm.right.every(v=>v===-.5));close();channel.port2.close();
  let Processor;
  runInNewContext(AUDIO_PROCESSOR_SOURCE,{AudioWorkletProcessor:class{port={};},sampleRate:48000,Float32Array,registerProcessor:(_,ctor)=>{Processor=ctor;}});
  const processor=new Processor(),requests=[];const link={postMessage:m=>requests.push(m),close:()=>{link.closed=true;}};
  processor.port.onmessage({data:{port:link}});
  const tick=n=>{const outputs=[[new Float32Array(n),new Float32Array(n)]];assert.equal(processor.process([],outputs),true);return outputs[0];};
  assert.ok(tick(128)[0].every(v=>v===0));tick(512);assert.equal(requests.length,1);
  link.onmessage({data:{left:new Float32Array(2048).fill(.25),right:new Float32Array(2048).fill(-.5)}});
  assert.ok(tick(1600)[0].every(v=>v===.25));assert.equal(requests.length,2);
  link.onmessage({data:{left:new Float32Array(2048).fill(.75),right:new Float32Array(2048).fill(.5)}});
  const crossing=tick(1024);assert.ok(crossing[0].slice(0,448).every(v=>v===.25));assert.ok(crossing[0].slice(448).every(v=>v===.75));
  tick(2048);assert.ok(tick(256)[0].every(v=>v===0));processor.port.onmessage({data:{stop:true}});
  assert.equal(processor.process([],[]),false);assert.equal(link.closed,true);
  console.log('DirectSound PCM Wasm boundary and original worklet transport: passed');
}
