let Resource = struct { value: i32 }
let Wrapper = struct { resource: Resource, value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let escape(): i32 = {
  let wrapper = Wrapper { resource: Resource { value: 0 }, value: return 42 }
  0
}

let main(): i32 = { escape() }
