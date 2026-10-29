import assert from 'node:assert/strict';

export async function keyboardSmoke(directory) {
  const { virtualKey } = await import(new URL('./keyboard.js', directory));
  for (const [code, value] of [['KeyP',80], ['Digit3',51], ['ArrowLeft',37],
    ['Equal',187], ['Minus',189], ['Comma',188], ['Period',190],
    ['F12',123], ['Numpad7',103], ['ControlLeft',17]]) {
    assert.equal(virtualKey({code, keyCode:0}),value);
  }
  assert.equal(virtualKey({keyCode:27}),27);
  assert.equal(virtualKey({code:'Unidentified',keyCode:0}),undefined);
  const { createDisplay } = await import(new URL('./presentation.js', directory));
  const handlers = new Map(), messages = [];
  const canvas = { addEventListener: (name, callback) => handlers.set(name, callback) };
  createDisplay(canvas, message => { messages.push(message); return true; }, {});
  let prevented = 0;
  const event = {code:'KeyP', keyCode:0, repeat:false, preventDefault(){prevented++;}};
  handlers.get('keydown')(event);
  handlers.get('keydown')({...event,repeat:true});
  handlers.get('blur')();
  assert.deepEqual(messages,[
    {kind:'key',code:80,down:true,repeat:false},
    {kind:'key',code:80,down:true,repeat:true},
    {kind:'key',code:80,down:false,repeat:false},
  ]);
  assert.equal(prevented,2);
  handlers.get('blur')(); assert.equal(messages.length,3);
  console.log('Keyboard: virtual-key mapping, repeats and blur release passed');
}
