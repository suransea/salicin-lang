let Result = std.Result
let FromUtf8Error = std.string.FromUtf8Error
let String = std.string.String
let Vec = std.vec.Vec

/// Validates and transfers ownership of bytes into an owning product name.
pub(package) let decode_name(move bytes: Vec(u8)): Result(FromUtf8Error)(String) = {
  String.from_utf8(bytes)
}
