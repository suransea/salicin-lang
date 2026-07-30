let choose(comptime t: type)(first: t)(second: t): t = { second }

let main(): i32 = {
  let choose_after_zero = choose(0)
  choose_after_zero(42)
}

test("infer_runtime_partial.sc") {
  std.test.assert(main() == 42)
}
