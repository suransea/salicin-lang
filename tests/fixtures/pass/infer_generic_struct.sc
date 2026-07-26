let cell(comptime t: type) = struct { value: t }

let main(): i32 = { cell { value: 42 }.value }

test("infer_generic_struct.sc") {
  main() == 42
}
