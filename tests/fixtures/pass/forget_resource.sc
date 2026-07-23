let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trap = 1 / self.value
  }}

let main(): i32 = {
  let resource = Resource { value: 0 }
  forget(resource)
  42
}
