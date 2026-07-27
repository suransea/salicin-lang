let add = std.ops.add

let number = struct { value: i32 }

extend(number, add(number)) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}

let main(): i32 = { 40 + 2 }

test("add_trait_builtin_integer_precedence.sc") {
  main() == 42
}
