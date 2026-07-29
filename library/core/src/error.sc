/// Typed non-local failure effect.
pub let throwing(comptime error: type) = effect {
  /// Raises `error` and does not return normally.
  let raise(move error: error): never
}

/// Handles `throwing(e)` from `action` and returns a `result`.
pub let try(comptime f: effects, comptime t: type, comptime e: type): with(f)(move action: with(core.error.throwing(e), f)((): t)): core.result(e)(t) = {
  core.error.throwing(e).handle
    raise { (error) -> core.result.err(error) }
    done { (value) -> core.result.ok(value) }
    action {
      action()
    }
}

/// Raises a value through `throwing(error)`.
pub let throw(comptime error: type): with(core.error.throwing(error))(move error: error): never = {
  core.error.throwing(error).raise(error)
}
