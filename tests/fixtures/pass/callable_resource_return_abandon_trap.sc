let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let finish(move resource: Resource)(value: i32): i32 = { value }

let make() = {
  let pending = finish(Resource { value: 0 })
  pending
}

let main(): i32 = {
  let pending = make()
  42
}
