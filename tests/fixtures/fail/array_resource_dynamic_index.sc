let resource = struct { value: i32 }

let main(): i32 = {
  let values = [resource { value: 42 }]
  let index = 0
  values[index].value
}
