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

/// Provides `?.` chaining for `Option`.
extend(T: type) Option(T): core.ops.Chain {
  /// The payload type produced by a successful option.
  let Item = T
  /// Rebuilds `Option` around a transformed payload type.
  let Rebind = Option

  /// Applies `transform` to `Some` and propagates `None`.
  let chain(E: effect, U: type)
    (move self)
    (move transform: (T): U with(E)): Option(U) with(E) = {
    self match {
      Some(value) => Option.Some(transform(value)),
      None => Option.None,
    }
  }
}

/// Provides `??` fallback evaluation for `Option`.
extend(T: type) Option(T): core.ops.Coalesce {
  /// The value type returned by coalescing.
  let Item = T

  /// Extracts `Some` or evaluates `fallback` for `None`.
  let coalesce(E: effect)
    (move self)
    (move fallback: (): T with(E)): T with(E) = {
    self match {
      Some(value) => value,
      None => fallback(),
    }
  }
}

/// Provides `?.` chaining for `Result`.
extend(Error: type, T: type) Result(Error)(T): core.ops.Chain {
  /// The success payload type.
  let Item = T
  /// Rebuilds `Result(Error)` around a transformed success type.
  let Rebind = Result(Error)

  /// Applies `transform` to `Ok` and propagates `Err`.
  let chain(E: effect, U: type)
    (move self)
    (move transform: (T): U with(E)): Result(Error)(U) with(E) = {
    self match {
      Ok(value) => Result.Ok(transform(value)),
      Err(error) => Result.Err(error),
    }
  }
}

/// Provides `??` fallback evaluation for `Result`.
extend(Error: type, T: type) Result(Error)(T): core.ops.Coalesce {
  /// The success payload type returned by coalescing.
  let Item = T

  /// Extracts `Ok` or evaluates `fallback` for `Err`.
  let coalesce(E: effect)
    (move self)
    (move fallback: (): T with(E)): T with(E) = {
    self match {
      Ok(value) => value,
      Err(_) => fallback(),
    }
  }
}
