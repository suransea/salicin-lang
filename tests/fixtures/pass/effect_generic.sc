let unsafety = core.unsafe.unsafety

let tagged(comptime e: effects): with(e)(value: i32): i32 = { value }
let forward(comptime e: effects): with(e)(value: i32): i32 = { tagged(e)(value) }

let main(): i32 = {
  forward(20) + forward(pure)(20) + unsafe { forward(e: unsafety)(2) }
}

test("effect_generic.sc") {
  std.test.assert(main() == 42)
}
