/// Represents either a present value or the absence of one.
pub let option(comptime t: type) = enum {
  /// Contains a value of type `T`.
  some(t),
  /// Contains no value.
  none,
}

/// Common inspection, borrowing, transformation, fallback, and conversion
/// operations for optional values.
extend(option(t)) {
  /// Returns whether this option contains a value.
  let is_some(self: borrow(self))(): bool = {
    match self
      { some(_) -> true }
      { none -> false }
  }

  /// Returns whether this option is empty.
  let is_none(self: borrow(self))(): bool = {
    match self
      { some(_) -> false }
      { none -> true }
  }

  /// Projects this borrowed option into an option of a payload borrow.
  let as_ref(comptime a: access, comptime r: region)
    (self: borrow(a)(r)(self))(): option(borrow(a)(r)(t)) = {
    match self
      { some(value) -> option.some(borrow(a)(value)) }
      { none -> option.none }
  }

  /// Transforms `some` once and preserves `none`.
  let map(comptime e: effects, comptime u: type): with(e)(move self)(move transform: with(e)((t): u)): option(u) = {
    match self
      { some(value) -> option.some(transform(value)) }
      { none -> option.none }
  }

  /// Runs `next` once for `some` and preserves `none`.
  let and_then(comptime e: effects, comptime u: type): with(e)(move self)(move next: with(e)((t): option(u))): option(u) = {
    match self
      { some(value) -> next(value) }
      { none -> option.none }
  }

  /// Extracts `some` or returns the eagerly evaluated fallback.
  let unwrap_or(move self)(move fallback: t): t = {
    match self
      { some(value) -> value }
      { none -> fallback }
  }

  /// Extracts `some` or evaluates `fallback` exactly once for `none`.
  let unwrap_or_else(comptime e: effects): with(e)(move self)(move fallback: with(e)((): t)): t = {
    match self
      { some(value) -> value }
      { none -> fallback() }
  }

  /// Converts `some` to `ok` and `none` to the eagerly evaluated error.
  let ok_or(comptime error: type)
    (move self)
    (move error: error): core.result(error)(t) = {
    match self
      { some(value) -> core.result.ok(value) }
      { none -> core.result.err(error) }
  }

  /// Converts `some` to `ok` and lazily constructs the error for `none`.
  let ok_or_else(comptime e: effects, comptime error: type): with(e)(move self)(move error: with(e)((): error)): core.result(error)(t) = {
    match self
      { some(value) -> core.result.ok(value) }
      { none -> core.result.err(error()) }
  }
}

/// Provides `?.` chaining for `Option`.
extend(option(t), core.flow.chain) {
  /// The payload type produced by a successful option.
  let item = t
  /// Rebuilds `Option` around a transformed payload type.
  let rebind = option

  /// Applies `transform` to `Some` and propagates `None`.
  let chain(comptime e: effects, comptime u: type): with(e)(self)(transform: with(e)((t): u)): option(u) = {
    match self
      { some(value) -> option.some(transform(value)) }
      { none -> option.none }
  }
}

/// Provides `??` fallback evaluation for `Option`.
extend(option(t), core.flow.coalesce) {
  /// The value type returned by coalescing.
  let item = t

  /// Extracts `Some` or evaluates `fallback` for `None`.
  let coalesce(comptime e: effects): with(e)(self)(fallback: with(e)((): t)): t = {
    match self
      { some(value) -> value }
      { none -> fallback() }
  }
}

/// Provides postfix `!` extraction for `Option`.
extend(option(t), core.flow.unwrap) {
  let output = t

  let unwrap(move self): t = {
    match self
      { some(value) -> value }
      { none -> unsafe { raw_trap() } }
  }
}
