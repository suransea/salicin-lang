let fail(): Result(i32, bool) = {
  throw
}

let main(): i32 = fail() ?? 42
