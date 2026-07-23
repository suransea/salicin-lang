/// Implements `Functor` for `Option`.
extend core.Option: core.functional.Functor {
  /// Maps `Some` through `transform` and preserves `None`.
  let map(E: effect, A: type, B: type)
    (move self: core.Option(A))
    (move transform: (A): B with(E)): core.Option(B) with(E) = {
    self match {
      Some(value) => core.Option.Some(transform(value)),
      None => core.Option.None,
    }
  }
}

/// Implements `Applicative` for `Option`.
extend core.Option: core.functional.Applicative {
  /// Wraps `value` in `Some`.
  let pure(A: type)
    (move value: A): core.Option(A) = {
    core.Option.Some(value)
  }

  /// Applies a `Some` function to a `Some` value and otherwise returns `None`.
  let apply(E: effect, A: type, B: type)
    (move self: core.Option((A): B with(E)))
    (move value: core.Option(A)): core.Option(B) with(E) = {
    self match {
      Some(transform) => value match {
        Some(value) => core.Option.Some(transform(value)),
        None => core.Option.None,
      },
      None => core.Option.None,
    }
  }
}

/// Implements `Monad` for `Option`.
extend core.Option: core.functional.Monad {
  /// Runs `next` for `Some` and propagates `None`.
  let flat_map(E: effect, A: type, B: type)
    (move self: core.Option(A))
    (move next: (A): core.Option(B) with(E)): core.Option(B) with(E) = {
    self match {
      Some(value) => next(value),
      None => core.Option.None,
    }
  }
}
