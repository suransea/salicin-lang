let Reset = trait {
  let reset(borrow(mut) self)(): i32
}

let Counter = struct { value: i32 }

extend Counter: Reset {
  let reset(borrow(mut) self)(): i32 = {
    self.value = 42
    self.value
  }
}

let main(): i32 = { Counter { value: 0 }.reset() }
