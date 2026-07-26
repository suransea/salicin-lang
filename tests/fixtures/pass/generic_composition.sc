let identity(comptime t: type)(move value: t): t = { value }

let wrap(comptime t: type)(move value: t): t = { identity(t)(value) }

let main(): i32 = { wrap(i32)(42) }

test("generic_composition.sc") {
  main() == 42
}
