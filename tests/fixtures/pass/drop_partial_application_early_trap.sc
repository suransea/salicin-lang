let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let finish(move resource: Resource)(value: i32): i32 = { value }

let main(): i32 = {
  let pending = finish(Resource { value: 0 })
  pending(return 0)
}
