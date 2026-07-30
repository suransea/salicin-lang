let option = core.option

let main(): i32 = {
  let answer = option.none ?? 42
  answer
}

test("coalesce_infer_local_without_annotation.sc") {
  std.test.assert(main() == 42)
}
