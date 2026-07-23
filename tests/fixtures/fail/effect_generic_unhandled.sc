use std.effect.Unsafe

let tagged(E: effect)(value: i32): i32 with(E) = { value }
let forward(E: effect)(value: i32): i32 with(E) = { tagged(E)(value) }

let main(): i32 = { forward(Unsafe)(42) }
