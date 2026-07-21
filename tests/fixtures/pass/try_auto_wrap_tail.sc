let option_value(): Option(i32) = 20
let result_value(): Result(i32, bool) = 22

let main(): i32 = (option_value() ?? 0) + (result_value() ?? 0)
