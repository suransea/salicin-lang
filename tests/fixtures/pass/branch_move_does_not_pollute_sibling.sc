let boxed = struct { value: i32 }

let consume(move boxed: boxed): () = {}

let main(): i32 = {
  let boxed = boxed { value: 42 }
  if true {
    consume(boxed)
  } else {
    let snapshot = boxed.value
  }
  42
}

test("branch_move_does_not_pollute_sibling.sc") {
  std.test.assert(main() == 42)
}
