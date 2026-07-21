let Counter = struct(value: i32)

extend Counter {
  let reset(borrow(mut) self)(): () = {
    self.value = 0
  }
}

let main(): i32 = {
  let counter = Counter(42)
  counter.reset()
  0
}
