let eq = std.ops.eq

let token = struct { value: i32 }

extend(token, eq(token)) {
  let eq(self: borrow(self))(rhs: borrow(token)): bool = { self.value == rhs.value }
}

let main(): i32 = {
  let left = token { value: 7 }
  let same = token { value: 7 }
  let different = token { value: 8 }
  if left == same && left != different { 42 } else { 0 }
}

test("eq_operator_trait.sc") {
  main() == 42
}
