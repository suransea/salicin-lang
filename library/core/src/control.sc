// A handler clause receives a compiler-created, one-shot value with this
// logical type. It may be invoked once to resume the suspended computation;
// dropping it aborts that continuation. Native lowering represents it with
// erased call/drop entries, an environment pointer, and a one-shot flag.
pub let Continuation(Input: type, Output: type) = struct()

// An owned, erased action that may perform a handled algebraic effect. Its
// native representation carries an action entry, a drop entry, an environment
// pointer, and an ownership flag. The entry consumes an `Input` and a
// `Continuation(Output, Answer)`; `Answer` is the surrounding handler result.
pub let EffectCallable(Input: type, Output: type, Answer: type) = struct()

// Control syntax uses trailing-closure call notation and lowers through these
// signatures. Their bodies are supplied by the compiler because they delimit
// or transform control flow rather than behaving like ordinary calls.
pub let do(E: effect, T: type)(move action: (): T with(E)): T with(E)
pub let try(F: effect, T: type, E: type)(move action: (): T with(throws(E), F)): Result(T, E) with(F)
pub let unsafe(E: effect, T: type)(move action: (): T with(unsafe, E)): T with(E)
pub let loop(E: effect, T: type)(move body: (): () with(E)): T with(E)
