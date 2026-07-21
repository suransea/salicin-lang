let convert(move value: Result(i32, bool)): Result(i32, i32) = value.try

let main(): i32 = convert(Result(i32, bool).Ok(42)) ?? 0
