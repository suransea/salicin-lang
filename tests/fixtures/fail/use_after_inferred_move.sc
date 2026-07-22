let Boxed = struct { value: i32 }

let consume(boxed: Boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = Boxed { value: 42 }
  let answer = consume(boxed)
  boxed.value + answer
}
