let identity(comptime t: type)(move value: t): t = { value }

let tick(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  42
}

let main(): i32 = {
  let mut count = 0
  let value = identity(tick(count))
  if count == 1 { value } else { 0 }
}

test("infer_argument_once.sc") {
  main() == 42
}
