let Result = std.Result
let Throws = std.error.Throws

let fail(E: type)(move error: E): i32 with(Throws(E)) = {
  throw(error)
}

let main(): i32 = {
  let result: Result(bool)(i32) = try { fail(bool)(true) }
  result ?? 42
}

test("throw_generic_error.sc") {
  main() == 42
}
