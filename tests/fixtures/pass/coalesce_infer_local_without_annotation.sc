let Option = std.Option

let main(): i32 = {
  let answer = Option.None ?? 42
  answer
}

test("coalesce_infer_local_without_annotation.sc") {
  main() == 42
}
