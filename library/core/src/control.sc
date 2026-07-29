// Control syntax uses trailing-closure call notation and targets these
// validated functions. Most control helpers are ordinary source definitions;
// the compiler only keeps syntax-directed shortcuts and the few places that
// need authority or primitive control-flow lowering.
/// Dynamically exits the nearest loop whose result type is `T`.
pub let loop_exit(comptime t: type) = effect {
  let exit(move value: t): never
}

/// Dynamically starts the next iteration of the nearest loop.
pub let iteration_skip = effect {
  let next(): never
}

/// Dynamically returns from the nearest function boundary returning `T`.
pub let function_exit(comptime t: type) = effect {
  let exit(move value: t): never
}

/// The observable result of trying one refutable pattern function.
pub let attempt(comptime input: type)(comptime output: type) = enum {
  hit(output),
  miss(input),
}

pub let break(comptime t: type): with(loop_exit(t))(move value: t): never = {
  loop_exit(t).exit(value)
}

pub let break: with(loop_exit(()))(): never = {
  loop_exit(()).exit(())
}

pub let continue: with(iteration_skip)(): never = {
  iteration_skip.next()
}

pub let return(comptime t: type): with(function_exit(t))(move value: t): never = {
  function_exit(t).exit(value)
}

pub let return: with(function_exit(()))(): never = {
  function_exit(()).exit(())
}

/// Runs `action` and preserves its effect row.
pub let do(comptime e: effects, comptime t: type): with(e)(move action: with(e)((): t)): t = {
  action()
}

/// Registers `action` to run when the current lexical scope exits.
pub let defer(comptime e: effects): with(e)(move action: with(e)((): ())): () = builtin()

/// Runs `action` once, then repeats it while the lazy condition remains true.
pub let do(comptime e: effects): with(e)(move action: with(core.control.loop_exit(()), core.control.iteration_skip, e)((): ()))(move while: with(core.control.loop_exit(()), core.control.iteration_skip, e)((): bool)): () = {
  loop {
    core.control.iteration_skip.handle
      next { () }
      action {
        action()
      }
    if while() {
      continue()
    } else {
      break()
    }
  }
}

/// Repeats `body` indefinitely until control exits through another construct.
pub let loop(comptime e: effects, comptime t: type): with(e)(move body: with(core.control.loop_exit(t), core.control.iteration_skip, e)((): ())): t = builtin()

/// Repeats `body` while the lazy condition remains true.
pub let while(comptime e: effects): with(e)(move condition: with(e)((): bool))(move do: with(e)((): ())): () = {
  loop {
    if condition() {
      do()
    } else {
      break()
    }
  }
}

/// Selects one of two lazy branches from an eager boolean condition.
pub let if(comptime e: effects, comptime t: type): with(e)(condition: bool)(move then: with(e)((): t))(move else: with(e)((): t)): t = {
  match condition
    { true -> then() }
    { false -> else() }
}

/// Selects the first matching case parameter group.
pub let match(
  comptime input: type,
  comptime output: type,
  comptime e: effects,
  comptime ...cases: parameters,
): with(e)
  (move input: input)
  ...cases: output = builtin()

/// Iterates through `iterable`, passing each item to the lazy body.
pub let for(comptime e: effects, comptime iterable: type, comptime iter: type, comptime item: type): with(e)(move iterable: iterable)(move body: with(core.control.loop_exit(()), core.control.iteration_skip, e)((item): ())): () = requires(
    iterable is core.iter.into_iterator &&
    iterable.iter == iter &&
    iter is core.iter.iterator &&
    iter.item == item
) {
  let mut iterator = iterable.into_iter()
  loop {
    match iterator.next()
      { some(item) -> body(item) }
      { none -> break() }
  }
}
