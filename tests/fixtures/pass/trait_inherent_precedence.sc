let Answer = trait {
  let answer(self: borrow(Self))(): i32
}

let Number = struct { value: i32 }

extend Number: Answer {
  let answer(self: borrow(Self))(): i32 = { 1 }
}

extend Number {
  let answer(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = {
  let number = Number { value: 42 }
  number.answer()
}
