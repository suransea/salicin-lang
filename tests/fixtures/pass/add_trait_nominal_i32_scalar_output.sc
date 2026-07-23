use std.ops.Add

let Number = struct { value: i32 }

extend Number: Add(i32) {
  let Output = i32
  let add(move self)(move rhs: i32): i32 = { self.value + rhs }
}

let main(): i32 = { Number { value: 40 } + 2 }
