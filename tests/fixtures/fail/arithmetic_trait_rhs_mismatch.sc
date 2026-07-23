use std.ops.Div

let Number = struct { value: i32 }
let Divisor = struct { value: i32 }

extend Number: Div(i32) {
  let Output = i32
  let div(move self)(move rhs: i32): i32 = { self.value / rhs }
}

let main(): i32 = { Number { value: 84 } / Divisor { value: 2 } }
