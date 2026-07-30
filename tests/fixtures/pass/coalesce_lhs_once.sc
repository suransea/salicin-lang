let option = core.option

let make(count: borrow(mut)(i32)): option(i32) = {
  count = count + 1
  option(i32).some(42)
}

let main(): i32 = {
  let mut count = 0
  let answer = make(count) ?? 0
  if count == 1 { answer } else { 0 }
}

test("coalesce_lhs_once.sc") {
  std.test.assert(main() == 42)
}
