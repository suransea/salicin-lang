let poll = core.async.poll
let future = core.async.future

let main(): i32 = {
  let mut future = async {
    await async { 42 }
  }
  match future.poll()
    { ready(value) -> value }
    { pending -> 0 }
}

test("async_await_ready.sc") {
  std.test.assert(main() == 42)
}
