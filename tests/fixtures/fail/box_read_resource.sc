use std.boxed.{Box, box_read}

let Resource = struct { value: i32 }

let main(): i32 = { box_read(T: Resource)(Box.new(Resource { value: 42 })).value }
