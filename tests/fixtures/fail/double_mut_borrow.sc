let boxed = struct { value: i32 }

let main(): i32 = {
  let mut boxed = boxed { value: 42 }
  let first = borrow(mut)(boxed)
  let second = borrow(mut)(boxed)
  first.value + second.value
}
