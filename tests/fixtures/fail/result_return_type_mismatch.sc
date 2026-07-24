let Result = std.Result

let wrong(): Result(bool)(i32) = { true }

let main(): i32 = { wrong() ?? 0 }
