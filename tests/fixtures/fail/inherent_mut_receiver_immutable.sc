let Counter = struct { value: i32 }

extend Counter {
  let reset(self: borrow(mut)(Self))(): () = {
    self.value = 0
  }
}

let main(): i32 = {
  let counter = Counter { value: 42 }
  counter.reset()
  0
}
