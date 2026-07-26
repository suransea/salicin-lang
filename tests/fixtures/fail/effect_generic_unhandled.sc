let Unsafe = std.unsafe.Unsafe

let tagged(E: effects)(value: i32): i32 with(E) = { value }
let forward(E: effects)(value: i32): i32 with(E) = { tagged(E)(value) }

let main(): i32 = { forward(Unsafe)(42) }
