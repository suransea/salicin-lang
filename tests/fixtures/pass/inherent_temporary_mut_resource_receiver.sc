let Resource = struct { value: i32 }

extend Resource {
  let increment(self: borrow(mut)(Self))(): i32 = {
    self.value = self.value + 1
    self.value
  }
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let main(): i32 = { Resource { value: 41 }.increment() }
