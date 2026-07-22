use core.Option
use core.Result

let Boxed = struct { value: i32 }

let option_value(): Option(i32) = { Option.Some(Boxed { value: 20 })?.value }

let result_value(): Result(bool)(i32) = { Result.Ok(Boxed { value: 22 })?.value }

let main(): i32 = { (option_value() ?? 0) + (result_value() ?? 0) }
