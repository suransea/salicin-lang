let String = std.string.String
let Vec = std.vec.Vec

let main(): i32 = {
  let bytes = Vec(u8).new()
  let text = String.from_utf8_unchecked(bytes)
  let length = text.len_bytes()
  0
}
