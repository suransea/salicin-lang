let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let cell: cell(i64) = cell { value: 42 }
  if cell.value == 42 { 42 } else { 0 }
}

test("infer_nominal_from_expected.sc") {
  std.test.assert(main() == 42)
}
