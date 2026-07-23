use std.ops.Sub

let Number = struct { value: i32 }

extend Number: Sub(i32) {
  let Output = i32
  let sub(self)(rhs: i32): i32 = { self.value - rhs }
}

extend Number: Sub(i64) {
  let Output = i64
  let sub(self)(rhs: i64): i64 = { 44 - rhs }
}

let main(): i32 = {
  let answer: i64 = Number { value: 40 } - 2
  if answer == 42 { 42 } else { 0 }
}
