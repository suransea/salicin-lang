let bad(R: region)(value: borrow(R)(i32)): borrow(mut, R)(i32) = { borrow(value) }
let main(): i32 = { 42 }
