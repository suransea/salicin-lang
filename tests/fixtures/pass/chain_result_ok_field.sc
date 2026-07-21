let Boxed = struct(value: i32)

let main(): i32 = Result(Boxed, bool).Ok(Boxed(42))?.value ?? 0
