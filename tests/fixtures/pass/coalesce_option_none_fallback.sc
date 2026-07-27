let option = core.option

let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  42
}

let main(): i32 = {
  let mut count = 0
  let answer = option(i32).none ?? fallback(count)
  if count == 1 { answer } else { 0 }
}

test("coalesce_option_none_fallback.sc") {
  main() == 42
}
