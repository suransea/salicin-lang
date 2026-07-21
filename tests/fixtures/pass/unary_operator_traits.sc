use core.ops.{Neg, Not}

let Number = struct(value: i32)
let Flag = struct(value: bool)

extend Number: Neg {
  let Output = i32
  let neg(move self)(): i32 = { -self.value }
}

extend Flag: Not {
  let Output = i32
  let not(move self)(): i32 = { if self.value { 0 } else { 42 } }
}

let negate(T: type)(move value: T): T where T: Neg(Output = T) = { -value }
let invert(T: type)(move value: T): T where T: Not(Output = T) = { !value }

let main(): i32 = { if invert(false) {
  !Flag(false) + -Number(0) + negate(0)
} else {
  0
}
}
