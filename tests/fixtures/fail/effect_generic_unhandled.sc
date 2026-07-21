let tagged(E: effect)(value: i32)(E): i32 = value
let forward(E: effect)(value: i32)(E): i32 = tagged(E)(value)

let main(): i32 = forward(unsafe)(42)
