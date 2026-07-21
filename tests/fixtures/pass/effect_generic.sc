let tagged(E: effect)(value: i32): i32(E) = value
let forward(E: effect)(value: i32): i32(E) = tagged(E)(value)

let main(): i32 = forward(20) + forward(pure)(20) + unsafe { forward(E: unsafe)(2) }
