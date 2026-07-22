let Boxed = struct { value: i32 }

let consume(move boxed: Boxed): i32 = { boxed.value }

let main(): i32 = {
  let mut boxed = Boxed { value: 0 }
  let mut iteration = 0
  while iteration < 2 {
    let previous = consume(boxed)
    boxed = Boxed { value: previous + 21 }
    iteration = iteration + 1
  }
  consume(boxed)
}
