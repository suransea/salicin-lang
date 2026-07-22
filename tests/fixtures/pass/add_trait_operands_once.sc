use core.ops.Add

let Number = struct { value: i32 }

extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { Number { value: self.value + rhs.value } }
}

let tick(borrow(mut) count: i32)(value: i32): Number = {
  count = count + 1
  Number { value: value }
}

let main(): i32 = {
  let mut left_count = 0
  let mut right_count = 0
  let answer = tick(left_count)(19) + tick(right_count)(23)
  if left_count == 1 && right_count == 1 { answer.value } else { 0 }
}
