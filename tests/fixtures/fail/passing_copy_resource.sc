let Token = struct(value: i32)

let identity(P: passing, T: type)(P value: T): T = value

let main(): i32 = {
  let token = Token(42)
  identity(copy, Token)(token).value
}
