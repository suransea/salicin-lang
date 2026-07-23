let Number = struct { value: i32 }

extend Number {
  let read(self: borrow(Self))(): i32 = { self.value }
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let number = Number { value: 21 }
  let first = number.read()
  first + number.take()
}
