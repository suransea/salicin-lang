let option = core.option

let main(): i32 = { option(i32).none ?? option.none ?? 42 }

test("coalesce_infer_right_associative_none.sc") {
  std.test.assert(main() == 42)
}
