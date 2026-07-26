let boxed = struct { value: i32 }

let read(boxed: borrow(boxed)): i32 = { boxed.value }
let consume(move boxed: boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = boxed { value: 42 }
  let snapshot = read(boxed)
  snapshot + consume(boxed) - 42
}

test("borrow_released_after_complete_call.sc") {
  main() == 42
}
