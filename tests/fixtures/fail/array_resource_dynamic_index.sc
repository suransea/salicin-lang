let Resource = struct { value: i32 }

let main(): i32 = {
  let values = [Resource { value: 42 }]
  let index = 0
  values[index].value
}
