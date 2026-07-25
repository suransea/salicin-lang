let Result = std.Result

let Boxed = struct { value: i32 }

let main(): i32 = { Result(bool)(Boxed).Ok(Boxed { value: 42 })?.value ?? 0 }

test("chain_result_ok_field.sc") {
  main() == 42
}
