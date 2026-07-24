let Result = std.Result

let Boxed = struct { value: i32 }

let main(): i32 = { Result(bool)(Boxed).Err(true)?.value ?? 42 }
