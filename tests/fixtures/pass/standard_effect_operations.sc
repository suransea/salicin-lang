use core.effects.{Throws, Async}

let fail_with_answer(): Never with(Throws(i32)) = {
  Throws(i32).raise(42)
}

let handled_async(): i32 = {
  let mut seen = 0
  let value = Async.handle(
    suspend: { (resume) ->
      seen = 1;
      resume(())
    },
  ) {
    Async.suspend();
    1
  }
  value + seen + 40
}

let main(): i32 = {
  handled_async()
}
