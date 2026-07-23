let LeftAnswer = trait {
  let answer(self: borrow(Self))(): i32
}

let RightAnswer = trait {
  let answer(self: borrow(Self))(): i32
}

let Number = struct { value: i32 }

extend Number: LeftAnswer {
  let answer(self: borrow(Self))(): i32 = { self.value }
}

extend Number: RightAnswer {
  let answer(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = {
  let number = Number { value: 42 }
  number.answer()
}
