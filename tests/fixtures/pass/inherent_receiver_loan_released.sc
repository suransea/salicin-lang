let number = struct { value: i32 }

extend number {
  let read(self: borrow(self))(): i32 = { self.value }
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let number = number { value: 21 }
  let first = number.read()
  first + number.take()
}

test("inherent_receiver_loan_released.sc") {
  main() == 42
}
