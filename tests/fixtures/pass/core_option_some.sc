let option = core.option

let main(): i32 = {
  let value = option(i32).some(42)
  match value
    { some(item) -> item }
    { none -> 0 }
}

test("core_option_some.sc") {
  std.test.assert(main() == 42)
}
