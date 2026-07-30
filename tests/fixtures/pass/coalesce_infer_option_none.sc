let option = core.option

let main(): i32 = { option.none ?? 42 }

test("coalesce_infer_option_none.sc") {
  std.test.assert(main() == 42)
}
