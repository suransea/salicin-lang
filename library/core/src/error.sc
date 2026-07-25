/// Typed non-local failure effect.
pub let Throws(Error: type) = effect {
  /// Raises `error` and does not return normally.
  let raise(move error: Error): Never
}

/// Handles `Throws(E)` from `action` and returns a `Result`.
pub let try(F: effect, T: type, E: type)
  (move action: (): T with(core.error.Throws(E), F)): core.Result(E)(T) with(F) = {
  core.error.Throws(E).handle
    raise { (error) -> core.Result.Err(error) }
    done { (value) -> core.Result.Ok(value) }
    action {
      action()
    }
}

/// Raises a value through `Throws(Error)`.
pub let throw(Error: type)
  (move error: Error): Never with(core.error.Throws(Error)) = {
  core.error.Throws(Error).raise(error)
}
