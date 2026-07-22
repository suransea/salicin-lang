let Counter = struct { value: i32 }

extend Counter {
  let reset(borrow(mut) self)(): () = {
    self.value = 0
  }

  let answer = 42
}

let main(): i32 = {
  let mut counter = Counter { value: 41 }
  counter.reset()
  counter.value + Counter.answer
}
