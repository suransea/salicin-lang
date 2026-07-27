/// Typed non-local failure effect.
pub let throwing(comptime error: type) = effect {
  /// Raises `error` and does not return normally.
  let raise(move error: error): never
}

/// Handles `throwing(e)` from `action` and returns a `result`.
pub let try(comptime f: effects, comptime t: type, comptime e: type)
  (move action: (): t with(core.error.throwing(e), f)): core.result(e)(t) with(f) = {
  core.error.throwing(e).handle
    raise { (error) -> core.result.err(error) }
    done { (value) -> core.result.ok(value) }
    action {
      action()
    }
}

/// Raises a value through `throwing(error)`.
pub let throw(comptime error: type)
  (move error: error): never with(core.error.throwing(error)) = {
  core.error.throwing(error).raise(error)
}
