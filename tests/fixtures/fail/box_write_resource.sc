use alloc.boxed.{Box, box_write}

let Resource = struct(value: i32)

let main(): i32 = {
  box_write(T: Resource)(Box.new(Resource(1)))(Resource(2))
  0
}
