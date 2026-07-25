let Add = std.ops.Add

let Number = struct { value: i32 }

extend Number: Add(Number) {
  let Output = Number
  let add(self)(rhs: Number): Number = { Number { value: self.value + rhs.value } }
}

let main(): i32 = { 40 + 2 }

test("add_trait_builtin_integer_precedence.sc") {
  main() == 42
}
