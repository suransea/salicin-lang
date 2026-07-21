let Resource = struct(value: i32)

let main(): i32 = {
  let value = Resource(42)
  forget(value)
  value.value
}
