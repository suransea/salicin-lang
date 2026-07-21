let Resource = struct(value: i32)

let main(): i32 = {
  let values = [Resource(42)]
  let index = 0
  values[index].value
}
