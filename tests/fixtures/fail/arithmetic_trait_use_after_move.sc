let rem = core.ops.rem

let number = struct { value: i32 }

extend(number, rem(number)) {
  let output = number
  let rem(self)(rhs: number): number = { number { value: self.value % rhs.value } }
}

let main(): i32 = {
  let left = number { value: 86 }
  let right = number { value: 44 }
  let answer = left % right
  left.value + right.value + answer.value
}
