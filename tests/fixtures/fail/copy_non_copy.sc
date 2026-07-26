let boxed = struct { value: i32 }

let inspect(copy boxed: boxed): i32 = { boxed.value }

let main(): i32 = { inspect(boxed { value: 42 }) }
