/// Trait used by `?.` to transform successful container payloads.
pub let chain = trait {
  /// Payload type read from the successful case.
  let item: type
  /// Type constructor used to rebuild the container with a new payload.
  let rebind(comptime value: type): type

  /// Applies `transform` to the successful payload or propagates the residual case.
  let chain(comptime e: effects, comptime u: type): with(e)(self)(transform: with(e)((item): u)): rebind(u)
}

/// Trait used by `??` to extract a value or evaluate a fallback.
pub let coalesce = trait {
  /// Payload type produced by coalescing.
  let item: type

  /// Returns the successful payload or evaluates `fallback`.
  let coalesce(comptime e: effects): with(e)(self)(fallback: with(e)((): item)): item
}

/// Trait used by postfix `!!` to assert success and extract a payload.
pub let unwrap = trait {
  /// Payload type produced by unwrapping.
  let output: type

  /// Returns the successful payload or terminates when no payload is present.
  let unwrap(move self): output
}

/// Trait used by postfix `!` to turn a stored failure into `Throws`.
pub let raise = trait {
  /// Successful payload type.
  let output: type
  /// Error type introduced into the effect row.
  let error: type

  /// Returns the successful payload or raises the stored error.
  let raise: with(core.error.throwing(error))(move self): output
}
