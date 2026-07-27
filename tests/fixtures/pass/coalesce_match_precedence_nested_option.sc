let option = core.option

let main(): i32 = {
  let inner = option(i32).some(42)
  let outer = option(option(i32)).some(inner)
  match outer ?? option(i32).none
    { some(value) -> value }
    { none -> 0 }
}

test("coalesce_match_precedence_nested_option.sc") {
  main() == 42
}
