let Resource = struct { value: i32 }

let main(): i32 = {
  let value = Resource { value: 42 }
  forget(value)
  value.value
}
