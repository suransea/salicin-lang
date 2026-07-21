let Boxed = struct(value: i32)

let main(): i32 = {
  let mut boxed = Boxed(value: 42)
  let first = borrow(mut) boxed
  let second = borrow(mut) boxed
  first.value + second.value
}
