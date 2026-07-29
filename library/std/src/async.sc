/// Minimal allocation-free executor that polls one future until completion.
pub let spin = struct {}

extend(spin, core.async.executor) {
  let run(comptime e: effects, comptime f: type, comptime t: type): with(e)(self: borrow(mut)(self))(move future: f): t = requires(f is core.async.future(e) && f.output == t) {
    let mut current = future
    loop {
      match current.poll()
        { ready(value) -> break(value) }
        { pending -> continue() }
    }
  }
}
