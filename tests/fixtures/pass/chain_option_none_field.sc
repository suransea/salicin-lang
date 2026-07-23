use std.Option

let Boxed = struct { value: i32 }

let main(): i32 = { Option(Boxed).None?.value ?? 42 }
