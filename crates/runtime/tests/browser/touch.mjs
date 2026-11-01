import assert from 'node:assert/strict';

export async function touchSmoke(directory) {
  const { createDisplay } = await import(new URL('./display/presentation.js', directory));
  const previousSetTimeout = globalThis.setTimeout;
  const previousClearTimeout = globalThis.clearTimeout;
  const timers = new Map();
  let now = 0, nextTimer = 0;
  globalThis.setTimeout = (callback, delay) => {
    const id = ++nextTimer;
    timers.set(id, { callback, at: now + delay });
    return id;
  };
  globalThis.clearTimeout = id => timers.delete(id);
  const advance = elapsed => {
    now += elapsed;
    for (const [id, timer] of timers) {
      if (timer.at <= now) {
        timers.delete(id);
        timer.callback();
      }
    }
  };
  const finger = (identifier, clientX = 30, clientY = 40) => ({ identifier, clientX, clientY });
  const mouse = (buttons, x = 40, y = 40) => ({ kind: 'mouse', x, y, buttons });
  const setup = () => {
    const handlers = new Map(), messages = [];
    const canvas = {
      width: 800, height: 600,
      getBoundingClientRect: () => ({ left: 10, top: 20, width: 400, height: 300 }),
      addEventListener: (type, handler) => handlers.set(type, handler),
      focus() {}, setPointerCapture() {},
    };
    const display = createDisplay(canvas, message => { messages.push(message); return true; }, {});
    const touch = (type, ...targetTouches) => {
      let prevented = false;
      handlers.get(type)({ targetTouches, preventDefault() { prevented = true; } });
      assert.ok(prevented, 'native touch gestures must not scroll or synthesize clicks');
    };
    return { handlers, messages, display, touch };
  };

  try {
    // Real devices report separate starts even when the user intends one two-finger gesture.
    for (const delay of [0, 40, 120, 240]) {
      for (const liftedFirst of [1, 2]) {
        const { touch, messages } = setup();
        touch('touchstart', finger(1));
        advance(delay);
        touch('touchmove', finger(1, 32, 42));
        touch('touchstart', finger(2, 90, 100), finger(1, 32, 42));
        advance(300);
        assert.equal(messages.at(-1).buttons, 2, 'staggered fingers must press right');
        assert.ok(messages.every(message => message.buttons !== 1), 'no initial left click');
        touch('touchmove', finger(1, 40, 50), finger(2, 100, 110));
        assert.deepEqual(messages.at(-1), mouse(2, 60, 60), 'right drag follows the first finger');
        touch('touchend', finger(liftedFirst === 1 ? 2 : 1));
        touch('touchmove', finger(liftedFirst === 1 ? 2 : 1));
        touch('touchend');
        advance(300);
        assert.equal(messages.at(-1).buttons, 0);
        assert.ok(messages.every(message => message.buttons !== 1), 'remaining finger must not click');
        messages.length = 0;
        touch('touchstart', finger(3));
        touch('touchend');
        assert.deepEqual(messages.slice(-2), [mouse(1), mouse(0)], 'next gesture can left click');
      }
    }

    {
      const { touch, messages } = setup();
      touch('touchstart', finger(1), finger(2, 90, 100));
      touch('touchend');
      advance(300);
      assert.deepEqual(messages, [mouse(2), mouse(0)], 'simultaneous two-finger tap');
    }

    {
      const { touch, messages } = setup();
      touch('touchstart', finger(1));
      advance(80);
      touch('touchend');
      assert.deepEqual(messages.slice(-2), [mouse(1), mouse(0)], 'quick taps click on release');
      const count = messages.length;
      advance(300);
      assert.equal(messages.length, count, 'tap must not leave a delayed press');
    }

    {
      const { touch, messages } = setup();
      touch('touchstart', finger(1));
      touch('touchmove', finger(1, 50, 60));
      advance(249);
      assert.ok(messages.every(message => message.buttons === 0), 'allow a second finger during movement');
      advance(1);
      assert.deepEqual(messages.slice(-2), [mouse(1), mouse(1, 80, 80)], 'single drag starts at its original point');
      touch('touchmove', finger(1, 60, 70));
      assert.deepEqual(messages.at(-1), mouse(1, 100, 100));
      touch('touchend');
      assert.deepEqual(messages.at(-1), mouse(0, 100, 100));
    }

    for (const cancel of ['touchcancel', 'blur', 'reset']) {
      for (const held of [false, true]) {
        const { touch, handlers, display, messages } = setup();
        touch('touchstart', finger(1));
        if (held) advance(250);
        if (cancel === 'reset') display.reset();
        else if (cancel === 'blur') handlers.get('blur')();
        else touch('touchcancel');
        assert.equal(messages.at(-1).buttons, 0, 'cancellation releases held buttons');
        const count = messages.length;
        advance(300);
        assert.equal(messages.length, count, `${cancel} cancels delayed input`);
        if (!held) assert.ok(messages.every(message => message.buttons === 0), 'cancellation must not click');
      }
    }

    {
      const { touch, messages } = setup();
      touch('touchstart', finger(1));
      advance(250);
      touch('touchstart', finger(1), finger(2));
      assert.deepEqual(messages.slice(-2), [mouse(0), mouse(2)], 'late second finger releases left before right');
      touch('touchcancel');
      assert.equal(messages.at(-1).buttons, 0);
    }

    {
      const { handlers, messages } = setup();
      const pointer = { ...finger(1), pointerId: 1, buttons: 1, preventDefault() {} };
      handlers.get('pointerdown')({ ...pointer, pointerType: 'touch' });
      assert.deepEqual(messages, [], 'touch pointer events must not duplicate native touches');
      for (const pointerType of ['mouse', 'pen']) {
        handlers.get('pointerdown')({ ...pointer, pointerType });
        assert.deepEqual(messages.at(-1), mouse(1), 'mouse and pen presses remain immediate');
        handlers.get('pointerup')({ ...pointer, pointerType, buttons: 0 });
      }
    }

    assert.equal(timers.size, 0, 'all gestures clean up their recognition timers');
  } finally {
    globalThis.setTimeout = previousSetTimeout;
    globalThis.clearTimeout = previousClearTimeout;
  }
  console.log('Touch: staggered two-finger gestures, taps, drags, cancellation and pointer input passed');
}
