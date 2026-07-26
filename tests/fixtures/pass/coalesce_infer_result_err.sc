let result = std.result

let main(): i32 = { result(comptime e: bool).err(false) ?? 42 }

test("coalesce_infer_result_err.sc") {
  main() == 42
}
