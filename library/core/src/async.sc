/// Internal suspension effect discharged by compiler-generated futures.
pub let Async = effect {
  /// Suspends the current asynchronous computation.
  let suspend(): ()
}

/// Result of polling an asynchronous computation once.
pub let Poll(T: type) = enum {
  Pending,
  Ready(T)
}

/// A cold asynchronous computation with residual effect row `E`.
pub let Future(E: effect) = trait
where Self: Move {
  let Output: type

  let poll(R: region)
  (self: borrow(mut)(R)(Self))
  (): Poll(Output) with(E)
}

/// Explicit executor protocol. Creating a future never selects an executor.
pub let Executor = trait {
  let run(E: effect, F: type, T: type)
  (self: borrow(mut)(Self))
  (move future: F): T with(E)
  where F: Future(E, Output = T)
}

/// Minimal allocation-free executor that polls one future until completion.
pub let Spin = struct {}

extend Spin: Executor {
  let run(E: effect, F: type, T: type)
  (self: borrow(mut)(Self))
  (move future: F): T with(E)
  where F: Future(E, Output = T) = {
    let mut current = future
    loop {
      match current.poll()
      { Ready(value) -> break(value) }
      { Pending -> continue() }
    }
  }
}

/// Constructs a cold compiler-generated future without running `action`.
pub let async(E: effect, F: type, T: type)
(move action: (): T with(core.async.Async, E)): F
where F: Future(E, Output = T) = builtin()

/// Suspends the enclosing async computation until `future` is ready.
pub let await(E: effect, F: type, T: type)
(move future: F): T with(core.async.Async, E)
where F: Future(E, Output = T) = {
  let mut current = future
  loop {
    match current.poll()
    { Pending -> Async.suspend() }
    { Ready(value) -> break(value) }
  }
}
