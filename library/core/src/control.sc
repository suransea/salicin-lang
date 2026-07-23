/// One-shot value passed to a handler clause for resuming suspended work.
/// It may be invoked once to resume the suspended computation; dropping it
/// aborts that continuation. Native lowering represents it with erased
/// call/drop entries, an environment pointer, and a one-shot flag.
pub let Continuation(Input: type, Output: type) = struct {}

/// Owned, erased action that may perform a handled algebraic effect.
/// Its native representation carries an action entry, a drop entry, an
/// environment pointer, and an ownership flag. The entry consumes an `Input`
/// and a `Continuation(Output, Answer)`; `Answer` is the handler result.
pub let EffectCallable(Input: type, Output: type, Answer: type) = struct {}

/// Protocol anchor for compiler-derived effect handlers.
/// Every source `effect` declaration automatically satisfies this trait; the
/// operation clauses and `handle` member are synthesized from that operation set.
pub let Handle = trait(Self: effect) {
  /// Clause record type accepted by the synthesized handler for `Self`.
  let Clauses(Value: type, Answer: type): type
  /// Handles `Self` around `action`, leaving `Rest` as the residual effect row.
  let handle(Value: type, Answer: type, Rest: effect)
    (move clauses: Clauses(Value, Answer))
    (move action: (): Value with(Self, Rest)): Answer with(Rest)
}

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
  (move action: (): T with(core.effects.Throws(E), F)): core.Result(E)(T) with(F) = {
  core.effects.Throws(E).handle(
    raise: { (error) -> core.Result.Err(error) },
    done: { (value) -> core.Result.Ok(value) },
  ) {
    action()
  }
}

/// Raises a value through the standard `Throws(Error)` effect.
pub let throw(Error: type)
  (move error: Error): Never with(core.effects.Throws(Error)) = {
  core.effects.Throws(Error).raise(error)
}

/// Runs an action that requires the standard unsafe authority effect.
pub let unsafe(E: effect, T: type)
  (move action: (): T with(core.effects.Unsafe, E)): T with(E) = {
  core.effects.Unsafe.handle() {
    action()
  }
}

/// Repeats `body` indefinitely until control exits through another construct.
pub let loop(E: effect, T: type)
  (move body: (): () with(E)): T with(E)
