/// Trait used by `?.` to transform successful container payloads.
pub let Chain = trait {
  /// Payload type read from the successful case.
  let Item: type
  /// Type constructor used to rebuild the container with a new payload.
  let Rebind(Value: type): type

  /// Applies `transform` to the successful payload or propagates the residual case.
  let chain(E: effect, U: type)
    (self)
    (transform: (Item): U with(E)): Rebind(U) with(E)
}

/// Trait used by `??` to extract a value or evaluate a fallback.
pub let Coalesce = trait {
  /// Payload type produced by coalescing.
  let Item: type

  /// Returns the successful payload or evaluates `fallback`.
  let coalesce(E: effect)
    (self)
    (fallback: (): Item with(E)): Item with(E)
}

/// Trait used by postfix `!!` to assert success and extract a payload.
pub let Unwrap = trait {
  /// Payload type produced by unwrapping.
  let Output: type

  /// Returns the successful payload or terminates when no payload is present.
  let unwrap(move self): Output
}

/// Trait used by postfix `!` to turn a stored failure into `Throws`.
pub let Raise = trait {
  /// Successful payload type.
  let Output: type
  /// Error type introduced into the effect row.
  let Error: type

  /// Returns the successful payload or raises the stored error.
  let raise(move self): Output with(core.effect.Throws(Error))
}
