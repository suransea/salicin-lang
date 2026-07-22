let Value = struct { value: i32 }

let main(): i32 = {
  let mut value = Value { value: 40 }
  value += 2
  value.value
}
