/// Minimal allocation-free executor that polls one future until completion.
pub let spin = struct {}

extend(spin, core.async.executor) {
  let run(comptime e: effects, comptime f: type, comptime t: type)
    (self: borrow(mut)(self))
    (move future: f): t with(e)
  where f: core.async.future(e, output = t) = {
    let mut current = future
    loop {
      match current.poll()
        { ready(value) -> break(value) }
        { pending -> continue() }
    }
  }
}
