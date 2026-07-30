test("arithmetic") {
  std.test.assert(20 + 22 == 42)
}

test("control flow") {
  let value = if true { 40 } else { 0 }
  std.test.assert(value + 2 == 42)
}

test("ordinary helpers") {
  std.test.assert(helper() == 42)
}

let helper(): i32 = {
  42
}
