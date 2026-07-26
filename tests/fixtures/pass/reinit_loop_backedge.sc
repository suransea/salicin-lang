let boxed = struct { value: i32 }

let consume(move boxed: boxed): i32 = { boxed.value }

let main(): i32 = {
  let mut boxed = boxed { value: 0 }
  let mut iteration = 0
  while { iteration < 2 } {
    let previous = consume(boxed)
    boxed = boxed { value: previous + 21 }
    iteration = iteration + 1
  }
  consume(boxed)
}

test("reinit_loop_backedge.sc") {
  main() == 42
}
