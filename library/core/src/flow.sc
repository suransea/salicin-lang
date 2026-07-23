/// Trait used by `?.` to transform successful container payloads.
pub let Chain = trait {
  /// Payload type read from the successful case.
  let Item: type
  /// Type constructor used to rebuild the container with a new payload.
  let Rebind(Value: type): type

  /// Applies `transform` to the successful payload or propagates the residual case.
  let chain(E: effect, U: type)
    (move self)
    (move transform: (Item): U with(E)): Rebind(U) with(E)
}

/// Trait used by `??` to extract a value or evaluate a fallback.
pub let Coalesce = trait {
  /// Payload type produced by coalescing.
  let Item: type

  /// Returns the successful payload or evaluates `fallback`.
  let coalesce(E: effect)
    (move self)
    (move fallback: (): Item with(E)): Item with(E)
}

/// Trait used by postfix `!` to assert success and extract a payload.
pub let Unwrap = trait {
  /// Payload type produced by unwrapping.
  let Output: type

  /// Returns the successful payload or terminates when no payload is present.
  let unwrap(move self): Output
}
