let unsafety = std.unsafe.unsafety

let tagged(comptime e: effects)(value: i32): i32 with(e) = { value }
let forward(comptime e: effects)(value: i32): i32 with(e) = { tagged(e)(value) }

let main(): i32 = {
  forward(20) + forward(pure)(20) + unsafe { forward(e: unsafety)(2) }
}

test("effect_generic.sc") {
  main() == 42
}
