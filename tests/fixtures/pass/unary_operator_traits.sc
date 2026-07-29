let neg = core.ops.neg
let not = core.ops.not

let number = struct { value: i32 }
let flag = struct { value: bool }

extend(number, neg) {
  let output = i32
  let neg(self)(): i32 = { -self.value }}

extend(flag, not) {
  let output = i32
  let not(self)(): i32 = {
    if self.value { 0 } else { 42 }
  }
}

let negate(comptime t: type)(move value: t): t = requires(t is neg && t.output == t) { -value }
let invert(comptime t: type)(move value: t): t = requires(t is not && t.output == t) { !value }

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
