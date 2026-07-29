let resource = struct { value: i32 }

let main(): i32 = {
  let mut values: array(resource)(1) = [resource { value: 42 }]
  values.fill(resource { value: 0 })
  0
}
