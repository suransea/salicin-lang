let Counter = struct { value: i32 }

extend Counter {
  let reset(self: borrow(mut)(Self))(): i32 = {
    self.value = 42
    self.value
  }
}

let main(): i32 = { Counter { value: 0 }.reset() }
