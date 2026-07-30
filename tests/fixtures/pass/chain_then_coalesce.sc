let option = core.option

let boxed = struct { value: i32 }

let main(): i32 = { option(boxed).some(boxed { value: 42 })?.value ?? 0 }

test("chain_then_coalesce.sc") {
  std.test.assert(main() == 42)
}
