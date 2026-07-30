let poll = core.async.poll
let future = core.async.future

let child() = {
  async { 1 }
}

let main(): i32 = {
  let value = 41
  let reference: borrow(i32) = borrow(value)
  let mut future = async {
    let awaited = await child()
    reference + awaited
  }
  match future.poll()
    { ready(result) -> result }
    { pending -> 0 }
}

test("async_await_external_borrow.sc") {
  std.test.assert(main() == 42)
}
