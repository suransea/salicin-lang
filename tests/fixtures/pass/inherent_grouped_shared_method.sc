let Number = struct { value: i32 }

extend Number {
  let plus(self: borrow(Self))(x: i32)(y: i32): i32 = { self.value + x + y }
}

let main(): i32 = {
  let number = Number { value: 40 }
  number.plus(1)(1)
}
