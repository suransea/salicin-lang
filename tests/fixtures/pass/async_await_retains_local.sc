let poll = std.async.poll
let future = std.async.future

let child() = {
  async { 1 }
}

let main(): i32 = {
  let mut future = async {
    let first = 41
    let copy = first
    let second = await child()
    copy + second
  }
  match future.poll()
    { ready(value) -> value }
    { pending -> 0 }
}

test("async_await_retains_local.sc") {
  main() == 42
}
