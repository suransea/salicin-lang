let Value = struct { value: i32 }

let main(): i32 = {
  for Value { value: 42 } { value ->
    value
  }
  0
}
