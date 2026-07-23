/// Represents either a present value or the absence of one.
pub let Option(T: type) = enum {
  /// Contains a value of type `T`.
  Some(T),
  /// Contains no value.
  None,
}

/// Represents either a successful value or an error payload.
pub let Result(E: type)
  (T: type) = enum {
  /// Contains the successful value.
  Ok(T),
  /// Contains the error value.
  Err(E),
}
