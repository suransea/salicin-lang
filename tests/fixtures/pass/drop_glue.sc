let Resource = struct { value: i32 }
let Wrapper = struct { resource: Resource }
let Choice = enum {
  Some(Wrapper),
  None,
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    self.value = 0
  }}

let main(): i32 = {
  let value = Choice.Some(Wrapper { resource: Resource { value: 42 } })
  42
}
