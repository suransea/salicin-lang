let fail(E: type)(move error: E): Result(i32, E) = {
  throw error
}

let main(): i32 = fail(bool)(true) ?? 42
