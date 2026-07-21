let Boxed = struct(value: i32)

let main(): i32 = { Boxed(42)?.value ?? 0 }
