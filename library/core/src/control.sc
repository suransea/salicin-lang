// A handler clause receives a compiler-created, one-shot value with this
// logical type. It may be invoked once to resume the suspended computation;
// dropping it aborts that continuation. Native lowering represents it with
// erased call/drop entries, an environment pointer, and a one-shot flag.
pub let Continuation(Input: type, Output: type) = struct {}

// An owned, erased action that may perform a handled algebraic effect. Its
// native representation carries an action entry, a drop entry, an environment
// pointer, and an ownership flag. The entry consumes an `Input` and a
// `Continuation(Output, Answer)`; `Answer` is the surrounding handler result.
pub let EffectCallable(Input: type, Output: type, Answer: type) = struct {}

// Protocol anchor for compiler-derived effect handlers. Every source
// `effect` declaration automatically satisfies this trait; the operation
// clauses and the `handle` member are synthesized from that declaration's
// operation set.
pub let Handle = trait(Self: effect) {
  let Clauses(Value: type, Answer: type): type
}

// Control syntax uses trailing-closure call notation and targets these
// validated functions. Most control helpers are ordinary source definitions;
// the compiler only keeps syntax-directed shortcuts and the few places that
// need authority or primitive control-flow lowering.
pub let do(E: effect, T: type)(move action: (): T with(E)): T with(E) = {
  action()
}

pub let try(F: effect, T: type, E: type)(move action: (): T with(core.effects.Throws(E), F)): core.Result(E)(T) with(F) = {
  core.effects.Throws(E).handle(
    raise: { (error) -> core.Result.Err(error) },
    done: { (value) -> core.Result.Ok(value) },
  ) {
    action()
  }
}

pub let throw(Error: type)(move error: Error): Never with(core.effects.Throws(Error)) = {
  core.effects.Throws(Error).raise(error)
}

pub let unsafe(E: effect, T: type)(move action: (): T with(core.effects.Unsafe, E)): T with(E) = {
  core.effects.Unsafe.handle() {
    action()
  }
}
pub let loop(E: effect, T: type)(move body: (): () with(E)): T with(E)
