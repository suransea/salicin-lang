let String = std.string.String
let Vec = std.vec.Vec

let main(): i32 = {
  let text = String { bytes: Vec(u8).new() }
  let length = text.len_bytes()
  0
}
