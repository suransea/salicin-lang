let escape(comptime r: region)
  (anchor: borrow(r)(i32)): borrow(r)(core.string.str) = {
  let bytes: array(u8)(1) = [65]
  let source: borrow(core.memory.slice(u8)) = borrow(bytes)
  match core.string.str.from_utf8(source)
    { some(text) -> text }
    { none -> unsafe { raw_trap() } }
}

let main(): i32 = { 0 }
