let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let cell = cell(i32) { value: 42 }
  cell.value
}

test("generic_struct.sc") {
  std.test.assert(main() == 42)
}
