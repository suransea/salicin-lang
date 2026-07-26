let sub_operator = std.ops.sub_operator
let mul_operator = std.ops.mul_operator
let div_operator = std.ops.div_operator
let rem_operator = std.ops.rem_operator

let number = struct { value: i32 }

extend number: sub_operator(number) {
  let output = number
  let sub(self)(rhs: number): number = { number { value: self.value - rhs.value } }
}

extend number: mul_operator(number) {
  let output = number
  let mul(self)(rhs: number): number = { number { value: self.value * rhs.value } }
}

extend number: div_operator(number) {
  let output = number
  let div(self)(rhs: number): number = { number { value: self.value / rhs.value } }
}

extend number: rem_operator(number) {
  let output = number
  let rem(self)(rhs: number): number = { number { value: self.value % rhs.value } }
}

let main(): i32 = {
  let subtraction = number { value: 50 } - number { value: 8 }
  let multiplication = number { value: 6 } * number { value: 7 }
  let division = number { value: 84 } / number { value: 2 }
  let remainder = number { value: 86 } % number { value: 44 }
  if subtraction.value == 42 && multiplication.value == 42 && division.value == 42 && remainder.value == 42 {
    42
  } else {
    0
  }
}

test("arithmetic_traits_nominal_dispatch.sc") {
  main() == 42
}
