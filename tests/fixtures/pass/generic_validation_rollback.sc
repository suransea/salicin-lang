let identity(comptime t: type)(move value: t): t = { value }

let helper(value: i32) = { identity(i32)(value) }

let preserve(comptime t: type)(move value: t): t = {
  helper(0)
  value
}

let main(): i32 = { preserve(i32)(42) }

test("generic_validation_rollback.sc") {
  std.test.assert(main() == 42)
}
