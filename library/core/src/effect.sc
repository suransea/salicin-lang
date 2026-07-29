/// One-shot value passed to a handler clause for resuming suspended work.
pub let continuation(comptime input: type, comptime output: type): type = builtin()

/// Owned, erased action that may perform a handled algebraic effect.
pub let effect_callable(comptime input: type, comptime output: type, comptime answer: type): type = builtin()

/// Protocol anchor for compiler-derived effect handlers.
/// Every source `effect` declaration automatically satisfies this trait; the
/// operation clauses and `handle` member are synthesized from that operation set.
pub let handle = trait(comptime self: effect) {
  /// Clause parameter schema synthesized from the operations of `Self`.
  let clauses(comptime value: type, comptime answer: type): parameters
  /// Handles `Self` around `action`, leaving `Rest` as the residual effect row.
  let handle(comptime value: type, comptime answer: type, comptime rest: effects):
  with(rest)
    ...clauses(value, answer)
    (move action: with(self, rest)((): value)): answer
}
