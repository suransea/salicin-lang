let option = core.option

let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  0
}

let main(): i32 = {
  let mut count = 0
  let answer = option(i32).some(42) ?? fallback(count)
  if count == 0 { answer } else { 0 }
}

test("coalesce_option_some_short_circuit.sc") {
  std.test.assert(main() == 42)
}
