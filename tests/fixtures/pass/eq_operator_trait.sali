use core.ops.Eq

let Token = struct(value: i32)

extend Token: Eq(Token) {
  let eq(borrow self)(borrow rhs: Token): bool = self.value == rhs.value
}

let main(): i32 = {
  let left = Token(7)
  let same = Token(7)
  let different = Token(8)
  if left == same && left != different { 42 } else { 0 }
}
