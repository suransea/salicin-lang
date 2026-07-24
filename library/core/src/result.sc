/// Represents either a successful value or an error payload.
pub let Result(E: type)
  (T: type) = enum {
  /// Contains the successful value.
  Ok(T),
  /// Contains the error value.
  Err(E),
}

/// Provides `?.` chaining for `Result`.
extend(Error: type, T: type) Result(Error)(T): core.flow.Chain {
  /// The success payload type.
  let Item = T
  /// Rebuilds `Result(Error)` around a transformed success type.
  let Rebind = Result(Error)

  /// Applies `transform` to `Ok` and propagates `Err`.
  let chain(E: effect, U: type)
    (self)
    (transform: (T): U with(E)): Result(Error)(U) with(E) = {
    match self
      { Ok(value) -> Result.Ok(transform(value)) }
      { Err(error) -> Result.Err(error) }
  }
}

/// Provides `??` fallback evaluation for `Result`.
extend(Error: type, T: type) Result(Error)(T): core.flow.Coalesce {
  /// The success payload type returned by coalescing.
  let Item = T

  /// Extracts `Ok` or evaluates `fallback` for `Err`.
  let coalesce(E: effect)
    (self)
    (fallback: (): T with(E)): T with(E) = {
    match self
      { Ok(value) -> value }
      { Err(_) -> fallback() }
  }
}

/// Provides postfix `!` extraction for `Result`.
extend(Error: type, T: type) Result(Error)(T): core.flow.Unwrap {
  let Output = T

  let unwrap(move self): T = {
    match self
      { Ok(value) -> value }
      { Err(_) -> unsafe { raw_trap() } }
  }
}

/// Provides postfix `!` effect raising for `Result`.
extend(E: type, T: type) Result(E)(T): core.flow.Raise {
  let Output = T
  let Error = E

  let raise(move self): T with(core.effect.Throws(E)) = {
    match self
      { Ok(value) -> value }
      { Err(error) -> core.effect.Throws(E).raise(error) }
  }
}

/// Implements `Functor` for `Result(Error)`.
extend(Error: type) Result(Error): core.functional.Functor {
  /// Maps `Ok` through `transform` and preserves `Err`.
  let map(E: effect, A: type, B: type)
    (self: Result(Error)(A))
    (transform: (A): B with(E)): Result(Error)(B) with(E) = {
    match self
      { Ok(value) -> Result.Ok(transform(value)) }
      { Err(error) -> Result.Err(error) }
  }
}

/// Implements `Applicative` for `Result(Error)`.
extend(Error: type) Result(Error): core.functional.Applicative {
  /// Wraps `value` in `Ok`.
  let pure(A: type)
    (value: A): Result(Error)(A) = {
    Result.Ok(value)
  }

  /// Applies an `Ok` function to an `Ok` value and propagates the first `Err`.
  let apply(E: effect, A: type, B: type)
    (self: Result(Error)((A): B with(E)))
    (value: Result(Error)(A)): Result(Error)(B) with(E) = {
    match self
      { Ok(transform) -> match value
        { Ok(value) -> Result.Ok(transform(value)) }
        { Err(error) -> Result.Err(error) } }
      { Err(error) -> Result.Err(error) }
  }
}

/// Implements `Monad` for `Result(Error)`.
extend(Error: type) Result(Error): core.functional.Monad {
  /// Runs `next` for `Ok` and propagates `Err`.
  let flat_map(E: effect, A: type, B: type)
    (self: Result(Error)(A))
    (next: (A): Result(Error)(B) with(E)): Result(Error)(B) with(E) = {
    match self
      { Ok(value) -> next(value) }
      { Err(error) -> Result.Err(error) }
  }
}
