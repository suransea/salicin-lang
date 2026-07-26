let add_operator = std.ops.add_operator

let number = struct { value: i32 }

extend number: add_operator(number) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}

let tick(count: borrow(mut)(i32))(value: i32): number = {
  count = count + 1
  number { value: value }
}

let main(): i32 = {
  let mut left_count = 0
  let mut right_count = 0
  let answer = tick(left_count)(19) + tick(right_count)(23)
  if left_count == 1 && right_count == 1 { answer.value } else { 0 }
}

test("add_trait_operands_once.sc") {
  main() == 42
}
