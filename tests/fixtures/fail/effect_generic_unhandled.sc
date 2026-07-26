let unsafe_effect = std.unsafe.unsafe_effect

let tagged(comptime e: effects)(value: i32): i32 with(e) = { value }
let forward(comptime e: effects)(value: i32): i32 with(e) = { tagged(e)(value) }

let main(): i32 = { forward(unsafe_effect)(42) }
