use std.boxed.box_new

let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let main(): i32 = {
  let boxed = box_new(Resource { value: 0 })
  0
}
