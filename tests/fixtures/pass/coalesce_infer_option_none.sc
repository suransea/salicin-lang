let Option = std.Option

let main(): i32 = { Option.None ?? 42 }

test("coalesce_infer_option_none.sc") {
  main() == 42
}
