let Value = struct(value: i32)

let main(): i32 = {
  let mut value = Value(40)
  value += 2
  value.value
}
