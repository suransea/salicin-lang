let bad(comptime r: region)(value: borrow(r)(i32)): borrow(mut, r)(i32) = { borrow(value) }
let main(): i32 = { 42 }
