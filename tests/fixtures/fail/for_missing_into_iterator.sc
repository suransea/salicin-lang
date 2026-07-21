let Value = struct(value: i32)

let main(): i32 = {
  for value in Value(42) {
    value
  }
  0
}
