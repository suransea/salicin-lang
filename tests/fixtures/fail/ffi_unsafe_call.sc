let c_abs(value: i32): i32 = foreign(c, "abs")

let main(): i32 = { c_abs(-42) }
