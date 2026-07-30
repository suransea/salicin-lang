let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let inner = cell(i32) { value: 42 }
  let outer = cell { value: inner }
  outer.value.value
}

test("infer_nested_generic_struct.sc") {
  std.test.assert(main() == 42)
}
