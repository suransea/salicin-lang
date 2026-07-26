let boxed = struct { value: i32 }

let main(): i32 = { boxed { value: 42 }?.value ?? 0 }
