/// Represents either a successful value or an error payload.
pub let result(comptime e: type)
  (comptime t: type) = enum {
  /// Contains the successful value.
  ok(t),
  /// Contains the error value.
  err(e),
}

/// Common inspection, borrowing, transformation, fallback, and projection
/// operations for success-or-error values.
extend(result(error)(t)) {
  /// Returns whether this result contains a success value.
  let is_ok(self: borrow(self))(): bool = {
    match self
      { ok(_) -> true }
      { err(_) -> false }
  }

  /// Returns whether this result contains an error.
  let is_err(self: borrow(self))(): bool = {
    match self
      { ok(_) -> false }
      { err(_) -> true }
  }

  /// Projects this borrowed result into borrowed success and error payloads.
  let as_ref(comptime a: access, comptime r: region)
    (self: borrow(a)(r)(self))
    (): result(borrow(a)(r)(error))(borrow(a)(r)(t)) = {
    match self
      { ok(value) -> result.ok(borrow(a)(value)) }
      { err(error) -> result.err(borrow(a)(error)) }
  }

  /// Transforms `ok` once and preserves `err`.
  let map(comptime e: effects, comptime u: type): with(e)(move self)(move transform: with(e)((t): u)): result(error)(u) = {
    match self
      { ok(value) -> result.ok(transform(value)) }
      { err(error) -> result.err(error) }
  }

  /// Transforms `err` once and preserves `ok`.
  let map_error(comptime e: effects, comptime mapped_error: type): with(e)(move self)(move transform: with(e)((error): mapped_error)): result(mapped_error)(t) = {
    match self
      { ok(value) -> result.ok(value) }
      { err(error) -> result.err(transform(error)) }
  }

  /// Runs `next` once for `ok` and preserves `err`.
  let and_then(comptime e: effects, comptime u: type): with(e)(move self)(move next: with(e)((t): result(error)(u))): result(error)(u) = {
    match self
      { ok(value) -> next(value) }
      { err(error) -> result.err(error) }
  }

  /// Extracts `ok` or returns the eagerly evaluated fallback.
  let unwrap_or(move self)(move fallback: t): t = {
    match self
      { ok(value) -> value }
      { err(_) -> fallback }
  }

  /// Extracts `ok` or evaluates `fallback` exactly once for `err`.
  let unwrap_or_else(comptime e: effects): with(e)(move self)(move fallback: with(e)((error): t)): t = {
    match self
      { ok(value) -> value }
      { err(error) -> fallback(error) }
  }

  /// Converts `ok` to `some` and `err` to `none`.
  let ok(move self)(): core.option(t) = {
    match self
      { ok(value) -> core.option.some(value) }
      { err(_) -> core.option.none }
  }

  /// Converts `err` to `some` and `ok` to `none`.
  let err(move self)(): core.option(error) = {
    match self
      { ok(_) -> core.option.none }
      { err(error) -> core.option.some(error) }
  }
}

/// Provides `?.` chaining for `Result`.
extend(result(error)(t), core.flow.chain) {
  /// The success payload type.
  let item = t
  /// Rebuilds `Result(Error)` around a transformed success type.
  let rebind = result(error)

  /// Applies `transform` to `Ok` and propagates `Err`.
  let chain(comptime e: effects, comptime u: type): with(e)(self)(transform: with(e)((t): u)): result(error)(u) = {
    match self
      { ok(value) -> result.ok(transform(value)) }
      { err(error) -> result.err(error) }
  }
}

/// Provides `??` fallback evaluation for `Result`.
extend(result(error)(t), core.flow.coalesce) {
  /// The success payload type returned by coalescing.
  let item = t

  /// Extracts `Ok` or evaluates `fallback` for `Err`.
  let coalesce(comptime e: effects): with(e)(self)(fallback: with(e)((): t)): t = {
    match self
      { ok(value) -> value }
      { err(_) -> fallback() }
  }
}

/// Provides postfix `!` extraction for `Result`.
extend(result(error)(t), core.flow.unwrap) {
  let output = t

  let unwrap(move self): t = {
    match self
      { ok(value) -> value }
      { err(_) -> unsafe { raw_trap() } }
  }
}

/// Provides postfix `!` effect raising for `Result`.
extend(result(e)(t), core.flow.raise) {
  let output = t
  let error = e

  let raise: with(core.error.throwing(e))(move self): t = {
    match self
      { ok(value) -> value }
      { err(error) -> core.error.throwing(e).raise(error) }
  }
}
