use alloc.boxed.Box

let Resource = struct(value: i32)

let main(): i32 = {
  let mut boxed = Box.new(Resource(20))
  let reference = boxed.as_ref()
  let previous = boxed.replace(Resource(22))
  reference.value + previous.value
}
