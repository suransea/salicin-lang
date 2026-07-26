let option = std.option

let main(): i32 = { option.none ?? 42 }

test("coalesce_infer_option_none.sc") {
  main() == 42
}
