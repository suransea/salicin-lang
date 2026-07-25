test("arithmetic") {
  20 + 22 == 42
}

test("control flow") {
  let value = if true { 40 } else { 0 }
  value + 2 == 42
}

test("ordinary helpers") {
  helper() == 42
}

let helper(): i32 = {
  42
}
