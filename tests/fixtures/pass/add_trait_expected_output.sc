use core.ops.Add

let Number = struct { value: i32 }

extend Number: Add(i32) {
  let Output = i32
  let add(move self)(move rhs: i32): i32 = { self.value + rhs }
}

extend Number: Add(i64) {
  let Output = i64
  let add(move self)(move rhs: i64): i64 = { rhs + 40 }
}

let main(): i32 = {
  let answer: i64 = Number { value: 40 } + 2
  if answer == 42 { 42 } else { 0 }
}
