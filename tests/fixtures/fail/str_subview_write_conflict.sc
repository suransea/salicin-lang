let main(): i32 = {
  let mut bytes: array(u8)(1) = [65]
  let source: borrow(core.memory.slice(u8)) = borrow(bytes)
  let view: borrow(core.string.str) = match core.string.str.from_utf8(source)
    { some(text) -> text }
    { none -> unsafe { raw_trap() } }
  let subview: borrow(core.string.str) = match view.get(0, 1)
    { some(text) -> text }
    { none -> unsafe { raw_trap() } }
  bytes[0] = 66
  if subview.len() == 1 { 42 } else { 0 }
}
