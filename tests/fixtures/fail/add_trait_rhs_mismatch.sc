use std.ops.Add

let Number = struct { value: i32 }
let Offset = struct { value: i32 }

extend Number: Add(i32) {
  let Output = i32
  let add(self)(rhs: i32): i32 = { self.value + rhs }
}

let main(): i32 = { Number { value: 40 } + Offset { value: 2 } }
