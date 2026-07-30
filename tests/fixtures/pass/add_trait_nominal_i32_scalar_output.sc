let add = core.ops.add

let number = struct { value: i32 }

extend(number, add(i32)) {
  let output = i32
  let add(self)(rhs: i32): i32 = { self.value + rhs }
}

let main(): i32 = { number { value: 40 } + 2 }

test("add_trait_nominal_i32_scalar_output.sc") {
  std.test.assert(main() == 42)
}
