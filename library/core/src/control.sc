// Control syntax uses trailing-closure call notation and targets these
// validated functions. Most control helpers are ordinary source definitions;
// the compiler only keeps syntax-directed shortcuts and the few places that
// need authority or primitive control-flow lowering.
/// Dynamically exits the nearest loop whose result type is `T`.
pub let break_effect(comptime t: type) = effect {
  let exit(move value: t): never
}

/// Dynamically starts the next iteration of the nearest loop.
pub let continue_effect = effect {
  let next(): never
}

/// Dynamically returns from the nearest function boundary returning `T`.
pub let return_effect(comptime t: type) = effect {
  let exit(move value: t): never
}

/// The observable result of trying one refutable pattern function.
pub let attempt(comptime input: type)(comptime output: type) = enum {
  hit(output),
  miss(input),
}

pub let break(comptime t: type)
  (move value: t): never with(break_effect(t)) = {
  break_effect(t).exit(value)
}

pub let break(): never with(break_effect(())) = {
  break_effect(()).exit(())
}

pub let continue(): never with(continue_effect) = {
  continue_effect.next()
}

pub let return(comptime t: type)
  (move value: t): never with(return_effect(t)) = {
  return_effect(t).exit(value)
}

pub let return(): never with(return_effect(())) = {
  return_effect(()).exit(())
}

/// Runs `action` and preserves its effect row.
pub let do(comptime e: effects, comptime t: type)
  (move action: (): t with(e)): t with(e) = {
  action()
}

/// Registers `action` to run when the current lexical scope exits.
pub let defer(comptime e: effects)
  (move action: (): () with(e)): () with(e) = builtin()

/// Runs `action` once, then repeats it while the lazy condition remains true.
pub let do(comptime e: effects)
  (move action: (): () with(core.control.break_effect(()), core.control.continue_effect, e))
  (move while: (): bool with(core.control.break_effect(()), core.control.continue_effect, e)): () with(e) = {
  loop {
    core.control.continue_effect.handle
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
pub let loop(comptime e: effects, comptime t: type)
  (move body: (): () with(core.control.break_effect(t), core.control.continue_effect, e)): t with(e) = builtin()

/// Repeats `body` while the lazy condition remains true.
pub let while(comptime e: effects)
  (move condition: (): bool with(e))
  (move do: (): () with(e)): () with(e) = {
  loop {
    if condition() {
      do()
    } else {
      break()
    }
  }
}

/// Selects one of two lazy branches from an eager boolean condition.
pub let if(comptime e: effects, comptime t: type)
  (condition: bool)
  (move then: (): t with(e))
  (move else: (): t with(e)): t with(e) = {
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
)
  (move input: input)
  ...cases: output with(e) = builtin()

/// Iterates through `iterable`, passing each item to the lazy body.
pub let for(comptime e: effects, comptime iterable: type, comptime iter: type, comptime item: type)
  (move iterable: iterable)
  (move body: (item): () with(core.control.break_effect(()), core.control.continue_effect, e)): () with(e)
where iterable: core.iter.into_iterator(into_iter = iter),
  iter: core.iter.iterator(item = item) = {
  let mut iterator = iterable.into_iter()
  loop {
    match iterator.next()
      { some(item) -> body(item) }
      { none -> break() }
  }
}
