use std.ops.Rem

let Number = struct { value: i32 }

extend Number: Rem(Number) {
  let Output = Number
  let rem(self)(rhs: Number): Number = { Number { value: self.value % rhs.value } }
}

let main(): i32 = {
  let left = Number { value: 86 }
  let right = Number { value: 44 }
  let answer = left % right
  left.value + right.value + answer.value
}
