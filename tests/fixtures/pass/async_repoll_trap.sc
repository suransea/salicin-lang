let future = core.async.future

let main(): i32 = {
  let mut future = async { 42 }
  let first = future.poll()
  let second = future.poll()
  0
}

test("async_repoll_trap.sc") {
  main() == 42
}
