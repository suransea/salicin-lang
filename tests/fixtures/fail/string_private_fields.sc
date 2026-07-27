let string = alloc.string.string
let vec = alloc.vec.vec

let main(): i32 = {
  let text = string { bytes: vec(u8).new() }
  let length = text.len_bytes()
  0
}
