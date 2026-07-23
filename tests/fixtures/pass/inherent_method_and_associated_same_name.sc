let Number = struct { raw: i32 }

extend Number {
  let value(self: borrow(Self))(): i32 = { self.raw }
  let value = 2
}

let main(): i32 = {
  let number = Number { raw: 40 }
  number.value() + Number.value
}
