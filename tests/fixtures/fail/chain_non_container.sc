let Boxed = struct { value: i32 }

let main(): i32 = { Boxed { value: 42 }?.value ?? 0 }
