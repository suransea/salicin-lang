let unsafety = core.unsafe.unsafety

let tagged(comptime e: effects): with(e)(value: i32): i32 = { value }
let forward(comptime e: effects): with(e)(value: i32): i32 = { tagged(e)(value) }

let main(): i32 = { forward(unsafety)(42) }
