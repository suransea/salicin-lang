let fail(): Result(i32, bool) = {
  throw 42
}

let main(): i32 = fail() ?? 42
