let mul = core.ops.mul

let number = struct { value: i32 }

extend(number, mul(i32)) {
  let output = i32
  let mul(self)(rhs: i32): i32 = { self.value * rhs }
}

let main(): i32 = {
  let right = 2
  let answer = number { value: 21 } * right
  answer + right - 2
}

test("arithmetic_trait_scalar_rhs_auto_reuse.sc") {
  std.test.assert(main() == 42)
}
