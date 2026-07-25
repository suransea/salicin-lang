let Result = std.Result

let main(): i32 = { Result(E: bool).Err(false) ?? 42 }

test("coalesce_infer_result_err.sc") {
  main() == 42
}
