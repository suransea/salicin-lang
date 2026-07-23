// Control syntax uses trailing-closure call notation and targets these
// validated functions. Most control helpers are ordinary source definitions;
// the compiler only keeps syntax-directed shortcuts and the few places that
// need authority or primitive control-flow lowering.
/// Runs `action` and preserves its effect row.
pub let do(E: effect, T: type)
  (move action: (): T with(E)): T with(E) = {
  action()
}

/// Handles `Throws(E)` from `action` and returns a `Result`.
pub let try(F: effect, T: type, E: type)
  (move action: (): T with(core.effect.Throws(E), F)): core.Result(E)(T) with(F) = {
  core.effect.Throws(E).handle(
    raise: { (error) -> core.Result.Err(error) },
    done: { (value) -> core.Result.Ok(value) },
  ) {
    action()
  }
}

/// Raises a value through the standard `Throws(Error)` effect.
pub let throw(Error: type)
  (move error: Error): Never with(core.effect.Throws(Error)) = {
  core.effect.Throws(Error).raise(error)
}

/// Runs an action that requires the standard unsafe authority effect.
pub let unsafe(E: effect, T: type)
  (move action: (): T with(core.effect.Unsafe, E)): T with(E) = {
  core.effect.Unsafe.handle() {
    action()
  }
}

/// Repeats `body` indefinitely until control exits through another construct.
pub let loop(E: effect, T: type)
  (move body: (): () with(E)): T with(E)
