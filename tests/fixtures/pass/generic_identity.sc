let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = { identity(i32)(40) + identity(i32)(2) }

test("generic_identity.sc") {
  main() == 42
}
