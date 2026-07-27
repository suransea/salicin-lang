let result = core.result

let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  0
}

let main(): i32 = {
  let mut count = 0
  let answer = result(bool)(i32).ok(42) ?? fallback(count)
  if count == 0 { answer } else { 0 }
}

test("coalesce_result_ok_short_circuit.sc") {
  main() == 42
}
