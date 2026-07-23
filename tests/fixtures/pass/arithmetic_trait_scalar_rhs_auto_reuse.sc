use std.ops.Mul

let Number = struct { value: i32 }

extend Number: Mul(i32) {
  let Output = i32
  let mul(self)(rhs: i32): i32 = { self.value * rhs }
}

let main(): i32 = {
  let right = 2
  let answer = Number { value: 21 } * right
  answer + right - 2
}
