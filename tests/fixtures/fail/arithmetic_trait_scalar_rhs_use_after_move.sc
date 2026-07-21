use core.ops.Mul

let Number = struct(value: i32)

extend Number: Mul(i32) {
  let Output = i32
  let mul(move self)(move rhs: i32): i32 = { self.value * rhs }
}

let main(): i32 = {
  let right = 2
  let answer = Number(21) * right
  answer + right
}
