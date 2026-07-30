let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let inner = cell(i32) { value: 42 }
  let outer = cell(cell(i32)) { value: inner }
  outer.value.value
}

test("generic_nested_struct.sc") {
  std.test.assert(main() == 42)
}
