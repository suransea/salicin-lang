/// Represents either a successful value or an error payload.
pub let result(comptime e: type)
  (comptime t: type) = enum {
  /// Contains the successful value.
  ok(t),
  /// Contains the error value.
  err(e),
}

/// Provides `?.` chaining for `Result`.
extend(comptime error: type, comptime t: type) result(error)(t): core.flow.chain_operator {
  /// The success payload type.
  let item = t
  /// Rebuilds `Result(Error)` around a transformed success type.
  let rebind = result(error)

  /// Applies `transform` to `Ok` and propagates `Err`.
  let chain(comptime e: effects, comptime u: type)
    (self)
    (transform: (t): u with(e)): result(error)(u) with(e) = {
    match self
      { ok(value) -> result.ok(transform(value)) }
      { err(error) -> result.err(error) }
  }
}

/// Provides `??` fallback evaluation for `Result`.
extend(comptime error: type, comptime t: type) result(error)(t): core.flow.coalesce_operator {
  /// The success payload type returned by coalescing.
  let item = t

  /// Extracts `Ok` or evaluates `fallback` for `Err`.
  let coalesce(comptime e: effects)
    (self)
    (fallback: (): t with(e)): t with(e) = {
    match self
      { ok(value) -> value }
      { err(_) -> fallback() }
  }
}

/// Provides postfix `!` extraction for `Result`.
extend(comptime error: type, comptime t: type) result(error)(t): core.flow.unwrap_operator {
  let output = t

  let unwrap(move self): t = {
    match self
      { ok(value) -> value }
      { err(_) -> unsafe { raw_trap() } }
  }
}

/// Provides postfix `!` effect raising for `Result`.
extend(comptime e: type, comptime t: type) result(e)(t): core.flow.raise_operator {
  let output = t
  let error = e

  let raise(move self): t with(core.error.throws(e)) = {
    match self
      { ok(value) -> value }
      { err(error) -> core.error.throws(e).raise(error) }
  }
}

/// Implements `Functor` for `Result(Error)`.
extend(comptime error: type) result(error): core.functional.functor {
  /// Maps `Ok` through `transform` and preserves `Err`.
  let map(comptime e: effects, comptime a: type, comptime b: type)
    (self: result(error)(a))
    (transform: (a): b with(e)): result(error)(b) with(e) = {
    match self
      { ok(value) -> result.ok(transform(value)) }
      { err(error) -> result.err(error) }
  }
}

/// Implements `Applicative` for `Result(Error)`.
extend(comptime error: type) result(error): core.functional.applicative {
  /// Wraps `value` in `Ok`.
  let pure(comptime a: type)
    (value: a): result(error)(a) = {
    result.ok(value)
  }

  /// Applies an `Ok` function to an `Ok` value and propagates the first `Err`.
  let apply(comptime e: effects, comptime a: type, comptime b: type)
    (self: result(error)((a): b with(e)))
    (value: result(error)(a)): result(error)(b) with(e) = {
    match self
      { ok(transform) -> match value
        { ok(value) -> result.ok(transform(value)) }
        { err(error) -> result.err(error) } }
      { err(error) -> result.err(error) }
  }
}

/// Implements `Monad` for `Result(Error)`.
extend(comptime error: type) result(error): core.functional.monad {
  /// Runs `next` for `Ok` and propagates `Err`.
  let flat_map(comptime e: effects, comptime a: type, comptime b: type)
    (self: result(error)(a))
    (next: (a): result(error)(b) with(e)): result(error)(b) with(e) = {
    match self
      { ok(value) -> next(value) }
      { err(error) -> result.err(error) }
  }
}
