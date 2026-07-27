let reset = trait {
  let reset(self: borrow(mut)(self))(): i32
}

let counter = struct { value: i32 }

extend(counter, reset) {
  let reset(self: borrow(mut)(self))(): i32 = {
    self.value = 42
    self.value
  }
}

let main(): i32 = { counter { value: 0 }.reset() }

test("trait_temporary_mut_receiver.sc") {
  main() == 42
}
