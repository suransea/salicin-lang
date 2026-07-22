let Boxed = struct { value: i32 }

let main(): i32 = { Result(Boxed, bool).Err(true)?.value ?? 42 }
