let Unsafe = std.unsafe.Unsafe

let tagged(E: effects)(value: i32): i32 with(E) = { value }
let forward(E: effects)(value: i32): i32 with(E) = { tagged(E)(value) }

let main(): i32 = {
  forward(20) + forward(pure)(20) + unsafe { forward(E: Unsafe)(2) }
}

test("effect_generic.sc") {
  main() == 42
}
