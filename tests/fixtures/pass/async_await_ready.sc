let poll = std.async.poll
let future = std.async.future

let main(): i32 = {
  let mut future = async {
    await async { 42 }
  }
  match future.poll()
    { ready(value) -> value }
    { pending -> 0 }
}

test("async_await_ready.sc") {
  main() == 42
}
