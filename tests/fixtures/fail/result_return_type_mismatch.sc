let result = core.result

let wrong(): result(bool)(i32) = { true }

let main(): i32 = { wrong() ?? 0 }
