/// One-shot value passed to a handler clause for resuming suspended work.
pub let Continuation(Input: type, Output: type): type

/// Owned, erased action that may perform a handled algebraic effect.
pub let EffectCallable(Input: type, Output: type, Answer: type): type

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
