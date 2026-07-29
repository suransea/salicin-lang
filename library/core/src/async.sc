/// Internal suspension effect discharged by compiler-generated futures.
pub let suspension = effect {
  /// Suspends the current asynchronous computation.
  let suspend(): ()
}

/// Result of polling an asynchronous computation once.
pub let poll(comptime t: type) = enum {
  pending,
  ready(t)
}

/// A cold asynchronous computation with residual effect row `E`.
pub let future(comptime e: effects) = trait(requires: self is movable) {
  let output: type

  let poll(comptime r: region): with(e)(self: borrow(mut)(r)(self))(): poll(output)
}

/// Explicit executor protocol. Creating a future never selects an executor.
pub let executor = trait {
  let run(comptime e: effects, comptime f: type, comptime t: type): with(e)(self: borrow(mut)(self))(move future: f): t = requires(f is future(e) && f.output == t)
}

/// Constructs a cold compiler-generated future without running `action`.
pub let async(comptime e: effects, comptime f: type, comptime t: type)(move action: with(core.async.suspension, e)((): t)): f = requires(f is future(e) && f.output == t) builtin()

/// Suspends the enclosing async computation until `future` is ready.
pub let await(comptime e: effects, comptime f: type, comptime t: type): with(core.async.suspension, e)(move future: f): t = requires(f is future(e) && f.output == t) {
  let mut current = future
  loop {
    match current.poll()
      { pending -> suspension.suspend() }
      { ready(value) -> break(value) }
  }
}
