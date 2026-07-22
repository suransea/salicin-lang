use alloc.boxed.Box

let Resource = struct { value: i32 }

let main(): i32 = {
  Box.new(Resource { value: 1 }).write(Resource { value: 2 })
  0
}
