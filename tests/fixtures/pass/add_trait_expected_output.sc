let add = core.ops.add

let number = struct { value: i32 }

extend(number, add(i32)) {
  let output = i32
  let add(self)(rhs: i32): i32 = { self.value + rhs }
}

extend(number, add(i64)) {
  let output = i64
  let add(self)(rhs: i64): i64 = { rhs + 40 }
}

let main(): i32 = {
  let answer: i64 = number { value: 40 } + 2
  if answer == 42 { 42 } else { 0 }
}

test("add_trait_expected_output.sc") {
  main() == 42
}
