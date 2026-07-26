let option = std.option
let result = std.result

let boxed = struct { value: i32 }

let option_value(): option(i32) = { option.some(boxed { value: 20 })?.value }

let result_value(): result(bool)(i32) = { result.ok(boxed { value: 22 })?.value }

let main(): i32 = { (option_value() ?? 0) + (result_value() ?? 0) }

test("chain_inferred_inputs.sc") {
  main() == 42
}
