let Boxed = struct(value: i32)

let inspect(copy boxed: Boxed): i32 = { boxed.value }

let main(): i32 = { inspect(Boxed(value: 42)) }
