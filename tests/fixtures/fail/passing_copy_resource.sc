let Token = struct { value: i32 }

let identity(M: (P: parameters): parameters, T: type)(M value: T): T = { value }

let main(): i32 = {
  let token = Token { value: 42 }
  identity(copy, Token)(token).value
}
