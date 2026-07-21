let Resource = struct(value: i32)

extend Resource {
  let increment(borrow(mut) self)(): i32 = {
    self.value = self.value + 1
    self.value
  }
}

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let main(): i32 = { Resource(41).increment() }
