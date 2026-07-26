let string = std.string.string
let vec = std.vec.vec

let main(): i32 = {
  let text = string { bytes: vec(u8).new() }
  let length = text.len_bytes()
  0
}
