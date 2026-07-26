/// Typed non-local failure effect.
pub let throws(comptime error: type) = effect {
  /// Raises `error` and does not return normally.
  let raise(move error: error): never
}

/// Handles `Throws(E)` from `action` and returns a `Result`.
pub let try(comptime f: effects, comptime t: type, comptime e: type)
  (move action: (): t with(core.error.throws(e), f)): core.result(e)(t) with(f) = {
  core.error.throws(e).handle
    raise { (error) -> core.result.err(error) }
    done { (value) -> core.result.ok(value) }
    action {
      action()
    }
}

/// Raises a value through `Throws(Error)`.
pub let throw(comptime error: type)
  (move error: error): never with(core.error.throws(error)) = {
  core.error.throws(error).raise(error)
}
