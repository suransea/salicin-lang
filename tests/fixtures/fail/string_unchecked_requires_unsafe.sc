let string = alloc.string.string
let vec = alloc.vec.vec

let main(): i32 = {
  let bytes = vec(u8).new()
  let text = string.from_utf8_unchecked(bytes)
  let length = text.len_bytes()
  0
}
