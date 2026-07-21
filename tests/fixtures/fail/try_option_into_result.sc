let convert(move value: Option(i32)): Result(i32, bool) = value.try

let main(): i32 = convert(Option(i32).Some(42)) ?? 0
