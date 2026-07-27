let option = core.option

let main(): i32 = {
  let value: option(i32) = option.none
  match value
    { some(_) -> 0 }
    { none -> 42 }
}

test("core_option_none.sc") {
  main() == 42
}
