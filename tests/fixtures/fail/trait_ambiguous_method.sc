let LeftAnswer = trait {
  let answer(borrow self)(): i32
}

let RightAnswer = trait {
  let answer(borrow self)(): i32
}

let Number = struct(value: i32)

extend Number: LeftAnswer {
  let answer(borrow self)(): i32 = self.value
}

extend Number: RightAnswer {
  let answer(borrow self)(): i32 = self.value
}

let main(): i32 = {
  let number = Number(42)
  number.answer()
}
