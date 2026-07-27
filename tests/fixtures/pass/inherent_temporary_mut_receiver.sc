let counter = struct { value: i32 }

extend(counter) {
  let reset(self: borrow(mut)(self))(): i32 = {
    self.value = 42
    self.value
  }
}

let main(): i32 = { counter { value: 0 }.reset() }

test("inherent_temporary_mut_receiver.sc") {
  main() == 42
}
