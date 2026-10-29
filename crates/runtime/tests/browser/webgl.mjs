// Run in an actual browser. Integer uploads must match the software renderer exactly.
export async function frameRendererSmoke(directory) {
  const { createFrameRenderer } = await import(new URL('./frame-renderer.js', directory));
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 256;
  const renderer = createFrameRenderer(canvas);
  assert(renderer.packed, 'WebGL2 is unavailable in this test browser');
  const gl = canvas.getContext('webgl2');
  const words = Uint16Array.from({length:65536}, (_,i)=>i);
  renderer.paint({width:256,height:256,format:16,pitch:512,pixels:new Uint8Array(words.buffer)});
  const actual = new Uint8Array(256*256*4);
  gl.readPixels(0,0,256,256,gl.RGBA,gl.UNSIGNED_BYTE,actual);
  for(let y=0;y<256;y++)for(let x=0;x<256;x++){
    const value=y*256+x,at=((255-y)*256+x)*4;
    const expected=[Math.floor((value>>>11)*255/31),Math.floor(((value>>>5)&63)*255/63),Math.floor((value&31)*255/31),255];
    assert(expected.every((v,i)=>actual[at+i]===v),`RGB565 parity ${value}: ${actual.slice(at,at+4)} vs ${expected}`);
  }
  canvas.width=3;canvas.height=2;
  const raw=new Uint8Array(17),v=new DataView(raw.buffer);
  const colors=[0xf800,0x07e0,0x001f,0xffff,0,0x1234];
  colors.forEach((color,i)=>v.setUint16(1+Math.floor(i/3)*8+(i%3)*2,color,true));
  const packed={width:3,height:2,format:16,pitch:8,pixels:raw.subarray(1)};
  renderer.paint(packed);
  const small=new Uint8Array(24);gl.readPixels(0,0,3,2,gl.RGBA,gl.UNSIGNED_BYTE,small);
  assert(small[0]===255&&small[1]===255&&small[2]===255,'padded bottom row');
  assert(small[12]===255&&small[13]===0&&small[14]===0,'padded top row and orientation');
  const rgba=new Uint8Array(32);
  for(let y=0;y<2;y++)for(let x=0;x<3;x++)rgba.set([10+y*30+x,20+y,30+x,255],y*16+x*4);
  const frame={width:3,height:2,format:32,pitch:16,pixels:rgba};
  renderer.paint(frame);gl.readPixels(0,0,3,2,gl.RGBA,gl.UNSIGNED_BYTE,small);
  assert(small[0]===40&&small[12]===10,'RGBA transition and padded row upload');
  assert(gl.getError()===gl.NO_ERROR,'WebGL error');
  const fallback=document.createElement('canvas');fallback.width=3;fallback.height=2;
  const context=fallback.getContext.bind(fallback);
  fallback.getContext=(kind,...args)=>kind==='webgl2'?null:context(kind,...args);
  const software=createFrameRenderer(fallback);
  assert(!software.packed,'software negotiation');software.paint(packed);
  const reference=context('2d').getImageData(0,0,3,2).data;
  renderer.paint(packed);gl.readPixels(0,0,3,2,gl.RGBA,gl.UNSIGNED_BYTE,small);
  for(let y=0;y<2;y++)for(let x=0;x<3;x++)for(let c=0;c<4;c++)assert(reference[(y*3+x)*4+c]===small[((1-y)*3+x)*4+c],'software fallback parity');
  const loss=gl.getExtension('WEBGL_lose_context');
  if(loss){
    const lost=new Promise(resolve=>canvas.addEventListener('webglcontextlost',resolve,{once:true}));
    loss.loseContext();await lost;renderer.paint(frame);
    const restored=new Promise(resolve=>canvas.addEventListener('webglcontextrestored',resolve,{once:true}));
    await new Promise(resolve=>setTimeout(resolve,100));loss.restoreContext();await restored;
    gl.readPixels(0,0,3,2,gl.RGBA,gl.UNSIGNED_BYTE,small);
    assert(small[0]===40&&small[12]===10,'restoration must repaint the latest frame without another guest present');
    assert(gl.getError()===gl.NO_ERROR,'context restoration');
  }
  renderer.clear();
  return 'PASS RGB565 all65536 colors, pitch/offset/orientation, RGBA fallback, context restoration';
}
