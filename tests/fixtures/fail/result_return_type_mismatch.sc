let wrong(): Result(i32, bool) = { true }

let main(): i32 = { wrong() ?? 0 }
