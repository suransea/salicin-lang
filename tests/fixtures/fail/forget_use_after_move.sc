let resource = struct { value: i32 }

let main(): i32 = {
  let value = resource { value: 42 }
  forget(value)
  value.value
}
