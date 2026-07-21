let tagged(E: effect)(value: i32): i32(E) = value
let forward(E: effect)(value: i32): i32(E) = tagged(E)(value)

let main(): i32 = forward(unsafe)(42)
