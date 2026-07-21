let pure(): i32 = 42

let invoke(action: (): i32 with(unsafe))(): i32 with(unsafe) = action()

let main(): i32 = unsafe { invoke(pure)() }
