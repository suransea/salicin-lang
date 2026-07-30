let option = core.option

let choose(value: option(i32)): i32 = {
  match value
    { some(found) -> found }
    { none -> 2 }
}

let main(): i32 = {
  choose(some(40)) + choose(none)
}

test("if_let.sc") {
  std.test.assert(main() == 42)
}
