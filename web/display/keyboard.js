// DOM keyCode is deprecated and differs between browsers for punctuation.
// Match the runtime's US virtual-key layout while retaining older callers.
const keys = {
  Backspace: 8,
  Tab: 9,
  Enter: 13,
  NumpadEnter: 13,
  ShiftLeft: 16,
  ShiftRight: 16,
  ControlLeft: 17,
  ControlRight: 17,
  AltLeft: 18,
  AltRight: 18,
  Pause: 19,
  CapsLock: 20,
  Escape: 27,
  Space: 32,
  PageUp: 33,
  PageDown: 34,
  End: 35,
  Home: 36,
  ArrowLeft: 37,
  ArrowUp: 38,
  ArrowRight: 39,
  ArrowDown: 40,
  Insert: 45,
  Delete: 46,
  NumLock: 144,
  ScrollLock: 145,
  NumpadMultiply: 106,
  NumpadAdd: 107,
  NumpadSubtract: 109,
  NumpadDecimal: 110,
  NumpadDivide: 111,
  Semicolon: 186,
  Equal: 187,
  Comma: 188,
  Minus: 189,
  Period: 190,
  Slash: 191,
  Backquote: 192,
  BracketLeft: 219,
  Backslash: 220,
  BracketRight: 221,
  Quote: 222,
}

export function virtualKey(event) {
  const { code } = event

  if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3)

  if (/^Digit[0-9]$/.test(code)) return code.charCodeAt(5)

  if (/^Numpad[0-9]$/.test(code)) return 96 + Number(code.at(-1))

  if (/^F([1-9]|1[0-2])$/.test(code)) return 111 + Number(code.slice(1))

  return keys[code] ?? (event.keyCode > 0 && event.keyCode < 256 ? event.keyCode : undefined)
}
