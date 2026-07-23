/// Provides `?.` chaining for `Result`.
extend(Error: type, T: type) core.Result(Error)(T): core.flow.Chain {
  /// The success payload type.
  let Item = T
  /// Rebuilds `Result(Error)` around a transformed success type.
  let Rebind = core.Result(Error)

  /// Applies `transform` to `Ok` and propagates `Err`.
  let chain(E: effect, U: type)
    (move self)
    (move transform: (T): U with(E)): core.Result(Error)(U) with(E) = {
    self match {
      Ok(value) => core.Result.Ok(transform(value)),
      Err(error) => core.Result.Err(error),
    }
  }
}

/// Provides `??` fallback evaluation for `Result`.
extend(Error: type, T: type) core.Result(Error)(T): core.flow.Coalesce {
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

/// Implements `Functor` for `Result(Error)`.
extend(Error: type) core.Result(Error): core.functional.Functor {
  /// Maps `Ok` through `transform` and preserves `Err`.
  let map(E: effect, A: type, B: type)
    (move self: core.Result(Error)(A))
    (move transform: (A): B with(E)): core.Result(Error)(B) with(E) = {
    self match {
      Ok(value) => core.Result.Ok(transform(value)),
      Err(error) => core.Result.Err(error),
    }
  }
}

/// Implements `Applicative` for `Result(Error)`.
extend(Error: type) core.Result(Error): core.functional.Applicative {
  /// Wraps `value` in `Ok`.
  let pure(A: type)
    (move value: A): core.Result(Error)(A) = {
    core.Result.Ok(value)
  }

  /// Applies an `Ok` function to an `Ok` value and propagates the first `Err`.
  let apply(E: effect, A: type, B: type)
    (move self: core.Result(Error)((A): B with(E)))
    (move value: core.Result(Error)(A)): core.Result(Error)(B) with(E) = {
    self match {
      Ok(transform) => value match {
        Ok(value) => core.Result.Ok(transform(value)),
        Err(error) => core.Result.Err(error),
      },
      Err(error) => core.Result.Err(error),
    }
  }
}

/// Implements `Monad` for `Result(Error)`.
extend(Error: type) core.Result(Error): core.functional.Monad {
  /// Runs `next` for `Ok` and propagates `Err`.
  let flat_map(E: effect, A: type, B: type)
    (move self: core.Result(Error)(A))
    (move next: (A): core.Result(Error)(B) with(E)): core.Result(Error)(B) with(E) = {
    self match {
      Ok(value) => next(value),
      Err(error) => core.Result.Err(error),
    }
  }
}
