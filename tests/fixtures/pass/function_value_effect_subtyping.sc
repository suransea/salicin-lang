use std.effect.Unsafe

let pure(): i32 = { 42 }

let invoke(action: (): i32 with(Unsafe))(): i32 with(Unsafe) = { action() }

let main(): i32 = { unsafe { invoke(pure)() } }
