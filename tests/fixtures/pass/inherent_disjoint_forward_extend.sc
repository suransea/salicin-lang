extend Number {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let Number = struct { value: i32 }

extend Number {
  let bonus = 2
}

let main(): i32 = {
  let number = Number { value: 40 }
  number.read() + Number.bonus
}
