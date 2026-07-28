let main(): i32 = {
  let bytes: array(u8)(1) = [65]
  let source: borrow(core.memory.slice(u8)) = borrow(bytes)
  raw_str(source)
  0
}
