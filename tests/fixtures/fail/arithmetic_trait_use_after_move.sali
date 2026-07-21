use core.ops.Rem

let Number = struct(value: i32)

extend Number: Rem(Number) {
  let Output = Number
  let rem(move self)(move rhs: Number): Number = Number(self.value % rhs.value)
}

let main(): i32 = {
  let left = Number(86)
  let right = Number(44)
  let answer = left % right
  left.value + right.value + answer.value
}
