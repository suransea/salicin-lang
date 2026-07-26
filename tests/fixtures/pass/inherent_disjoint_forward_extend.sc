extend number {
  let read(self: borrow(self))(): i32 = { self.value }
}

let number = struct { value: i32 }

extend number {
  let bonus = 2
}

let main(): i32 = {
  let number = number { value: 40 }
  number.read() + number.bonus
}

test("inherent_disjoint_forward_extend.sc") {
  main() == 42
}
