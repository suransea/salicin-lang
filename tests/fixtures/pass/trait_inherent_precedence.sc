let answer = trait {
  let answer(self: borrow(self))(): i32
}

let number = struct { value: i32 }

extend number: answer {
  let answer(self: borrow(self))(): i32 = { 1 }
}

extend number {
  let answer(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = {
  let number = number { value: 42 }
  number.answer()
}

test("trait_inherent_precedence.sc") {
  main() == 42
}
