let result = core.result

let boxed = struct { value: i32 }

let main(): i32 = { result(bool)(boxed).ok(boxed { value: 42 })?.value ?? 0 }

test("chain_result_ok_field.sc") {
  main() == 42
}
