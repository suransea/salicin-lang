use core.ops.Mul

let Number = struct(value: i32)

extend Number: Mul(Number) {
  let Output = Number
  let mul(move self)(move rhs: Number): Number = { Number(self.value * rhs.value) }
}

let tick(borrow(mut) count: i32)(value: i32): Number = {
  count = count + 1
  Number(value)
}

let main(): i32 = {
  let mut left_count = 0
  let mut right_count = 0
  let answer = tick(left_count)(6) * tick(right_count)(7)
  if left_count == 1 && right_count == 1 { answer.value } else { 0 }
}
