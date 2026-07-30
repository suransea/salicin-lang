let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let flag = cell(bool) { value: true }
  let answer = cell(i32) { value: 42 }
  if flag.value { answer.value } else { 0 }
}

test("generic_nominal_multiple_instances.sc") {
  std.test.assert(main() == 42)
}
