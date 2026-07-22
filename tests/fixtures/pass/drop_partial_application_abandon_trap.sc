let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }}

let finish(move resource: Resource)(value: i32): i32 = { value }

let main(): i32 = {
  let pending = finish(Resource { value: 0 })
  0
}
