let option = std.option

let main(): i32 = { option(i32).none ?? option.none ?? 42 }

test("coalesce_infer_right_associative_none.sc") {
  main() == 42
}
