let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let inspect(resource: borrow(Resource)): i32 = { resource.value }

let main(): i32 = { inspect(Resource { value: 42 }) }
