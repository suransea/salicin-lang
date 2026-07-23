use std.ops.Add

let Number = struct { value: i32 }

extend Number: Add(i32) {
  let Output = Number
  let add(move self)(move rhs: i32): Number = { Number { value: self.value + rhs } }
}

let main(): i32 = {
  let answer = Number { value: 40 } + 2
  answer.value
}
