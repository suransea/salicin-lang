let bad('a: region)(value: borrow('a)(i32)): borrow(mut, 'a)(i32) = { borrow(value) }
let main(): i32 = { 42 }
