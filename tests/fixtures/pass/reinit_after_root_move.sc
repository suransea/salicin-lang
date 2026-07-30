let boxed = struct { value: i32 }

let consume(move boxed: boxed): i32 = { boxed.value }

let main(): i32 = {
  let mut boxed = boxed { value: 14 }
  let first = consume(boxed)
  boxed = boxed { value: 14 }
  let read = boxed.value
  let second = consume(boxed)
  first + read + second
}

test("reinit_after_root_move.sc") {
  std.test.assert(main() == 42)
}
