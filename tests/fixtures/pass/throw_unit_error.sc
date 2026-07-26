let result = std.result
let throws = std.error.throws

let fail(): i32 with(throws(())) = {
  throw(())
}

let main(): i32 = {
  let result: result(())(i32) = try { fail() }
  result ?? 42
}

test("throw_unit_error.sc") {
  main() == 42
}
