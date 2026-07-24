let Box = std.boxed.Box

let Resource = struct { value: i32 }

let main(): i32 = { Box.new(Resource { value: 42 }).read().value }
