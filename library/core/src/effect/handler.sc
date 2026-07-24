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
  /// Clause parameter schema synthesized from the operations of `Self`.
  let Clauses(Value: type, Answer: type): parameters
  /// Handles `Self` around `action`, leaving `Rest` as the residual effect row.
  let handle(Value: type, Answer: type, Rest: effect)
    ...Clauses(Value, Answer)
    (move action: (): Value with(Self, Rest)): Answer with(Rest)
}
