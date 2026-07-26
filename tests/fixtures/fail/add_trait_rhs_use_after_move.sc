let add_operator = std.ops.add_operator

let number = struct { value: i32 }

extend number: add_operator(number) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}

let main(): i32 = {
  let right = number { value: 2 }
  let answer = number { value: 40 } + right
  right.value
}
