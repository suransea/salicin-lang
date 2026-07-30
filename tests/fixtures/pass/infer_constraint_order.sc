let accept_same(comptime t: type)(left: t, right: t): i32 = { 21 }
let accept(comptime t: type)(value: t): i32 = { 21 }

let main(): i32 = {
  let wide: i64 = 7
  let ordered = accept_same(0, wide)
  let arithmetic = accept(0 + wide)
  ordered + arithmetic
}

test("infer_constraint_order.sc") {
  std.test.assert(main() == 42)
}
