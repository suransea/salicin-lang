/// Represents either a present value or the absence of one.
pub let option(comptime t: type) = enum {
  /// Contains a value of type `T`.
  some(t),
  /// Contains no value.
  none,
}

/// Provides `?.` chaining for `Option`.
extend(comptime t: type) option(t): core.flow.chain {
  /// The payload type produced by a successful option.
  let item = t
  /// Rebuilds `Option` around a transformed payload type.
  let rebind = option

  /// Applies `transform` to `Some` and propagates `None`.
  let chain(comptime e: effects, comptime u: type)
    (self)
    (transform: (t): u with(e)): option(u) with(e) = {
    match self
      { some(value) -> option.some(transform(value)) }
      { none -> option.none }
  }
}

/// Provides `??` fallback evaluation for `Option`.
extend(comptime t: type) option(t): core.flow.coalesce {
  /// The value type returned by coalescing.
  let item = t

  /// Extracts `Some` or evaluates `fallback` for `None`.
  let coalesce(comptime e: effects)
    (self)
    (fallback: (): t with(e)): t with(e) = {
    match self
      { some(value) -> value }
      { none -> fallback() }
  }
}

/// Provides postfix `!` extraction for `Option`.
extend(comptime t: type) option(t): core.flow.unwrap {
  let output = t

  let unwrap(move self): t = {
    match self
      { some(value) -> value }
      { none -> unsafe { raw_trap() } }
  }
}

/// Implements `Functor` for `Option`.
extend option: core.functional.functor {
  /// Maps `Some` through `transform` and preserves `None`.
  let map(comptime e: effects, comptime a: type, comptime b: type)
    (self: option(a))
    (transform: (a): b with(e)): option(b) with(e) = {
    match self
      { some(value) -> option.some(transform(value)) }
      { none -> option.none }
  }
}

/// Implements `Applicative` for `Option`.
extend option: core.functional.applicative {
  /// Wraps `value` in `Some`.
  let pure(comptime a: type)
    (value: a): option(a) = {
    option.some(value)
  }

  /// Applies a `Some` function to a `Some` value and otherwise returns `None`.
  let apply(comptime e: effects, comptime a: type, comptime b: type)
    (self: option((a): b with(e)))
    (value: option(a)): option(b) with(e) = {
    match self
      { some(transform) -> match value
        { some(value) -> option.some(transform(value)) }
        { none -> option.none } }
      { none -> option.none }
  }
}

/// Implements `Monad` for `Option`.
extend option: core.functional.monad {
  /// Runs `next` for `Some` and propagates `None`.
  let flat_map(comptime e: effects, comptime a: type, comptime b: type)
    (self: option(a))
    (next: (a): option(b) with(e)): option(b) with(e) = {
    match self
      { some(value) -> next(value) }
      { none -> option.none }
  }
}
