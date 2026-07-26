let boxed = struct { value: i32 }

let consume(move boxed: boxed): i32 = { boxed.value }

let restore(select_first: bool): i32 = {
  let mut boxed = boxed { value: 0 }
  consume(boxed)
  if select_first {
    boxed = boxed { value: 20 }
  } else {
    boxed = boxed { value: 22 }
  }
  consume(boxed)
}

let main(): i32 = { restore(true) + restore(false) }

test("reinit_after_both_if_branches.sc") {
  main() == 42
}
