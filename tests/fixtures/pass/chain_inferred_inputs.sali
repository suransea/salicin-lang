let Boxed = struct(value: i32)

let option_value(): Option(i32) = Option.Some(Boxed(20))?.value

let result_value(): Result(i32, bool) = Result.Ok(Boxed(22))?.value

let main(): i32 = (option_value() ?? 0) + (result_value() ?? 0)
