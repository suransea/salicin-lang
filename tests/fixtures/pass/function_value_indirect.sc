let increment(value: i32): i32 = { value + 1 }

let apply(action: (i32): i32)(value: i32): i32 = { action(value) }

let main(): i32 = { apply(increment)(41) }
