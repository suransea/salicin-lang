let Boxed = struct { value: i32 }

let consume(move value: Boxed): i32 = { value.value }

let main(): i32 = {
  let boxed = Boxed { value: 42 }
  loop {
    break consume(boxed)
  }
}
