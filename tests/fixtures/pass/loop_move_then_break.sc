let boxed = struct { value: i32 }

let consume(move value: boxed): i32 = { value.value }

let main(): i32 = {
  let boxed = boxed { value: 42 }
  loop {
    break(consume(boxed))
  }
}

test("loop_move_then_break.sc") {
  std.test.assert(main() == 42)
}
