let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = {
  let identity_i32 = identity(i32)
  identity_i32(42)
}

test("generic_type_application_partial.sc") {
  main() == 42
}
