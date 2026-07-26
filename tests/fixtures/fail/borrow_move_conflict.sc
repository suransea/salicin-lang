let boxed = struct { value: i32 }

let consume(move boxed: boxed): i32 = { boxed.value }

let main(): i32 = {
  let boxed = boxed { value: 42 }
  let borrowed = borrow(boxed)
  let answer = consume(boxed)
  borrowed.value + answer - 42
}
