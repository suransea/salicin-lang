let counter = struct { value: i32 }

extend counter {
  let set(self: borrow(mut)(self))(value: i32)(extra: i32): i32 = {
    self.value = value
    self.value + extra
  }
}

let main(): i32 = {
  let set = counter { value: 0 }.set(40)
  set(2)
}
