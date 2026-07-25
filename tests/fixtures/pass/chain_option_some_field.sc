let Option = std.Option

let Boxed = struct { value: i32 }

let main(): i32 = { Option(Boxed).Some(Boxed { value: 42 })?.value ?? 0 }

test("chain_option_some_field.sc") {
  main() == 42
}
