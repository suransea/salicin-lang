let result = core.result

let main(): i32 = { result(e: bool).err(false) ?? 42 }

test("coalesce_infer_result_err.sc") {
  main() == 42
}
