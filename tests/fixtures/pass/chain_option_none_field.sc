let Option = std.Option

let Boxed = struct { value: i32 }

let main(): i32 = { Option(Boxed).None?.value ?? 42 }

test("chain_option_none_field.sc") {
  main() == 42
}
