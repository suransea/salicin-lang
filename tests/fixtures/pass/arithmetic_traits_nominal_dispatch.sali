use core.ops.{Sub, Mul, Div, Rem}

let Number = struct(value: i32)

extend Number: Sub(Number) {
  let Output = Number
  let sub(move self)(move rhs: Number): Number = Number(self.value - rhs.value)
}

extend Number: Mul(Number) {
  let Output = Number
  let mul(move self)(move rhs: Number): Number = Number(self.value * rhs.value)
}

extend Number: Div(Number) {
  let Output = Number
  let div(move self)(move rhs: Number): Number = Number(self.value / rhs.value)
}

extend Number: Rem(Number) {
  let Output = Number
  let rem(move self)(move rhs: Number): Number = Number(self.value % rhs.value)
}

let main(): i32 = {
  let subtraction = Number(50) - Number(8)
  let multiplication = Number(6) * Number(7)
  let division = Number(84) / Number(2)
  let remainder = Number(86) % Number(44)
  if subtraction.value == 42 && multiplication.value == 42 && division.value == 42 && remainder.value == 42 {
    42
  } else {
    0
  }
}
