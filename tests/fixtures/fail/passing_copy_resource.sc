let token = struct { value: i32 }

let identity(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = { value }

let main(): i32 = {
  let token = token { value: 42 }
  identity(copy, token)(token).value
}
