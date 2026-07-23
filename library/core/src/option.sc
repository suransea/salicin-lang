/// Represents either a present value or the absence of one.
pub let Option(T: type) = enum {
  /// Contains a value of type `T`.
  Some(T),
  /// Contains no value.
  None,
}

/// Provides `?.` chaining for `Option`.
extend(T: type) Option(T): core.flow.Chain {
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
extend(T: type) Option(T): core.flow.Coalesce {
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

/// Implements `Functor` for `Option`.
extend Option: core.functional.Functor {
  /// Maps `Some` through `transform` and preserves `None`.
  let map(E: effect, A: type, B: type)
    (move self: Option(A))
    (move transform: (A): B with(E)): Option(B) with(E) = {
    self match {
      Some(value) => Option.Some(transform(value)),
      None => Option.None,
    }
  }
}

/// Implements `Applicative` for `Option`.
extend Option: core.functional.Applicative {
  /// Wraps `value` in `Some`.
  let pure(A: type)
    (move value: A): Option(A) = {
    Option.Some(value)
  }

  /// Applies a `Some` function to a `Some` value and otherwise returns `None`.
  let apply(E: effect, A: type, B: type)
    (move self: Option((A): B with(E)))
    (move value: Option(A)): Option(B) with(E) = {
    self match {
      Some(transform) => value match {
        Some(value) => Option.Some(transform(value)),
        None => Option.None,
      },
      None => Option.None,
    }
  }
}

/// Implements `Monad` for `Option`.
extend Option: core.functional.Monad {
  /// Runs `next` for `Some` and propagates `None`.
  let flat_map(E: effect, A: type, B: type)
    (move self: Option(A))
    (move next: (A): Option(B) with(E)): Option(B) with(E) = {
    self match {
      Some(value) => next(value),
      None => Option.None,
    }
  }
}
