let Eq = std.ops.Eq

let Token = struct { value: i32 }

extend Token: Eq(Token) {
  let eq(self: borrow(Self))(rhs: borrow(Token)): bool = { self.value == rhs.value }
}

let main(): i32 = {
  let left = Token { value: 7 }
  let same = Token { value: 7 }
  let different = Token { value: 8 }
  if left == same && left != different { 42 } else { 0 }
}
