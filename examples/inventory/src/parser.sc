let result = std.result
let from_utf8_error = std.string.from_utf8_error
let string = std.string.string
let vec = std.vec.vec

/// Validates and transfers ownership of bytes into an owning product name.
pub(package) let decode_name(move bytes: vec(u8)): result(from_utf8_error)(string) = {
  string.from_utf8(bytes)
}
