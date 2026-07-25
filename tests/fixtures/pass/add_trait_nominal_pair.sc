let Add = std.ops.Add

let Number = struct { value: i32 }

extend Number: Add(Number) {
  let Output = Number
  let add(self)(rhs: Number): Number = { Number { value: self.value + rhs.value } }
}

let main(): i32 = {
  let answer = Number { value: 19 } + Number { value: 23 }
  answer.value
}

test("add_trait_nominal_pair.sc") {
  main() == 42
}
