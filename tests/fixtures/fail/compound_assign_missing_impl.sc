let value = struct { value: i32 }

let main(): i32 = {
  let mut value = value { value: 40 }
  value += 2
  value.value
}
