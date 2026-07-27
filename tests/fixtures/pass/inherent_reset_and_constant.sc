let counter = struct { value: i32 }

extend(counter) {
  let reset(self: borrow(mut)(self))(): () = {
    self.value = 0
  }

  let answer = 42
}

let main(): i32 = {
  let mut counter = counter { value: 41 }
  counter.reset()
  counter.value + counter.answer
}

test("inherent_reset_and_constant.sc") {
  main() == 42
}
