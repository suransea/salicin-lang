let sub = core.ops.sub

let number = struct { value: i32 }

extend(number, sub(i32)) {
  let output = i32
  let sub(self)(rhs: i32): i32 = { self.value - rhs }
}

extend(number, sub(i64)) {
  let output = i64
  let sub(self)(rhs: i64): i64 = { 44 - rhs }
}

let main(): i32 = {
  let answer: i64 = number { value: 40 } - 2
  if answer == 42 { 42 } else { 0 }
}

test("arithmetic_trait_expected_output.sc") {
  std.test.assert(main() == 42)
}
