let Resource = struct { value: i32 }

extend Resource {
  let read(self: borrow(Self))(): i32 = { self.value }
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let main(): i32 = { Resource { value: 42 }.read() }
