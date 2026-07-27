let option = core.option

let main(): i32 = {
  let first = option(i32).none
  let second = option(i32).some(42)
  first ?? second ?? 0
}

test("coalesce_right_associative.sc") {
  main() == 42
}
