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
