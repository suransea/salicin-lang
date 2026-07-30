let option = core.option

let boxed = struct { value: i32 }

let main(): i32 = { option(boxed).none?.value ?? 42 }

test("chain_option_none_field.sc") {
  std.test.assert(main() == 42)
}
