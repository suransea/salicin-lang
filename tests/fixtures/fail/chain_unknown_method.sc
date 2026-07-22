let Boxed = struct { value: i32 }

let main(): i32 = { Option(Boxed).Some(Boxed { value: 42 })?.missing() ?? 0 }
