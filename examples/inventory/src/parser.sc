let result = core.result
let from_utf8_error = alloc.string.from_utf8_error
let string = alloc.string.string
let vec = alloc.vec.vec

/// Validates and transfers ownership of bytes into an owning product name.
pub(package) let decode_name(move bytes: vec(u8)): result(from_utf8_error)(string) = {
  string.from_utf8(bytes)
}
