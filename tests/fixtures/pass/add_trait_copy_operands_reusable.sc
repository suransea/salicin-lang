use std.marker.Copy
use std.ops.Add

let Number = struct { value: i32 }

extend Number: Copy {}

extend Number: Add(Number) {
  let Output = Number
  let add(self)(rhs: Number): Number = {
    Number { value: self.value + rhs.value }
  }
}

let main(): i32 = {
  let left = Number { value: 10 }
  let right = Number { value: 11 }
  let answer = left + right
  left.value + right.value + answer.value
}
