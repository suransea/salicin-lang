let boxed = struct { value: i32 }

let consume(boxed: boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = boxed { value: 42 }
  let answer = consume(boxed)
  boxed.value + answer
}
