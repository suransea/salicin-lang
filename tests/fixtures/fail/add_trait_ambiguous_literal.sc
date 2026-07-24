let Add = std.ops.Add

let Number = struct { value: i32 }

extend Number: Add(i32) {
  let Output = i32
  let add(self)(rhs: i32): i32 = { self.value + rhs }
}

extend Number: Add(i64) {
  let Output = i64
  let add(self)(rhs: i64): i64 = { rhs + 40 }
}

let main(): i32 = {
  let answer = Number { value: 40 } + 2
  42
}
