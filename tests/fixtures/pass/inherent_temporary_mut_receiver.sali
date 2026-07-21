let Counter = struct(value: i32)

extend Counter {
  let reset(borrow(mut) self)(): i32 = {
    self.value = 42
    self.value
  }
}

let main(): i32 = Counter(0).reset()
