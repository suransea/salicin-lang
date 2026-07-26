let neg_operator = std.ops.neg_operator
let not_operator = std.ops.not_operator

let number = struct { value: i32 }
let flag = struct { value: bool }

extend number: neg_operator {
  let output = i32
  let neg(self)(): i32 = { -self.value }}

extend flag: not_operator {
  let output = i32
  let not(self)(): i32 = {
    if self.value { 0 } else { 42 }
  }
}

let negate(comptime t: type)(move value: t): t where t: neg_operator(output = t) = { -value }
let invert(comptime t: type)(move value: t): t where t: not_operator(output = t) = { !value }

let main(): i32 = {
  if invert(false) {
    !flag { value: false } + -number { value: 0 } + negate(0)
  } else {
    0
  }
}

test("unary_operator_traits.sc") {
  main() == 42
}
