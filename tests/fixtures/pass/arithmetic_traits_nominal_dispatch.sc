use std.ops.{Sub, Mul, Div, Rem}

let Number = struct { value: i32 }

extend Number: Sub(Number) {
  let Output = Number
  let sub(self)(rhs: Number): Number = { Number { value: self.value - rhs.value } }
}

extend Number: Mul(Number) {
  let Output = Number
  let mul(self)(rhs: Number): Number = { Number { value: self.value * rhs.value } }
}

extend Number: Div(Number) {
  let Output = Number
  let div(self)(rhs: Number): Number = { Number { value: self.value / rhs.value } }
}

extend Number: Rem(Number) {
  let Output = Number
  let rem(self)(rhs: Number): Number = { Number { value: self.value % rhs.value } }
}

let main(): i32 = {
  let subtraction = Number { value: 50 } - Number { value: 8 }
  let multiplication = Number { value: 6 } * Number { value: 7 }
  let division = Number { value: 84 } / Number { value: 2 }
  let remainder = Number { value: 86 } % Number { value: 44 }
  if subtraction.value == 42 && multiplication.value == 42 && division.value == 42 && remainder.value == 42 {
    42
  } else {
    0
  }
}
