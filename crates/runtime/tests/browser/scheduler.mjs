// Runs in Node and in a real browser worker (see crates/runtime/tests/browser/check-scheduler.py).
// A resolved yield is insufficient: timers and input must run before the guest
// stops producing work, including when several guest threads yield together.
export async function schedulerSmoke(yieldToHost) {
  const turns = [];
  await Promise.all(Array.from({ length: 20 }, (_, index) => yieldToHost().then(() => turns.push(index))));
  if (turns.some((value, index) => value !== index)) throw new Error('Yield order changed');

  const channel = new MessageChannel();
  let timers = 0, messages = 0, yields = 0;
  channel.port1.onmessage = () => messages++;
  const timer = setInterval(() => timers++, 10);
  const input = setInterval(() => channel.port2.postMessage(null), 10);
  const started = performance.now();
  try {
    while (performance.now() - started < 300) {
      // Model the two-millisecond guest slice, without relying on game assets.
      const deadline = performance.now() + 2;
      while (performance.now() < deadline) {}
      await yieldToHost();
      yields++;
    }
    if (timers < 2 || messages < 2) {
      throw new Error(`Yield starved host tasks: ${timers} timers, ${messages} messages`);
    }
    return { timers, messages, yields };
  } finally {
    clearInterval(timer);
    clearInterval(input);
    channel.port1.close();
    channel.port2.close();
  }
}
