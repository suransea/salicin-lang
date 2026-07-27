let mul = std.ops.mul

let number = struct { value: i32 }

extend(number, mul(number)) {
  let output = number
  let mul(self)(rhs: number): number = { number { value: self.value * rhs.value } }
}

let tick(count: borrow(mut)(i32))(value: i32): number = {
  count = count + 1
  number { value: value }
}

let main(): i32 = {
  let mut left_count = 0
  let mut right_count = 0
  let answer = tick(left_count)(6) * tick(right_count)(7)
  if left_count == 1 && right_count == 1 { answer.value } else { 0 }
}

test("arithmetic_trait_operands_once.sc") {
  main() == 42
}
