let result = std.result

let boxed = struct { value: i32 }

let main(): i32 = { result(bool)(boxed).err(true)?.value ?? 42 }

test("chain_result_err_field.sc") {
  main() == 42
}
