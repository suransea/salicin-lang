use std.boxed.{Box, box_write}

let Resource = struct { value: i32 }

let main(): i32 = {
  box_write(T: Resource)(Box.new(Resource { value: 1 }))(Resource { value: 2 })
  0
}
