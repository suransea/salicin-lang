let boxed = struct { value: i32 }

let consume(move boxed: boxed): i32 = { boxed.value }

let choose(take: bool): i32 = {
  let boxed = boxed { value: 42 }
  if take {
    return(consume(boxed))
  }
  boxed.value
}

let main(): i32 = { choose(false) }

test("move_then_return_preserves_other_branch.sc") {
  std.test.assert(main() == 42)
}
