let Option = std.Option

let main(): i32 = { Option(i32).None ?? Option.None ?? 42 }

test("coalesce_infer_right_associative_none.sc") {
  main() == 42
}
