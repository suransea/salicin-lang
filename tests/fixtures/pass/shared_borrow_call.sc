let boxed = struct { value: i32 }

let read(boxed: borrow(boxed)): i32 = { boxed.value }

let main(): i32 = {
  let boxed = boxed { value: 42 }
  let snapshot = read(boxed)
  snapshot + boxed.value - 42
}

test("shared_borrow_call.sc") {
  std.test.assert(main() == 42)
}
