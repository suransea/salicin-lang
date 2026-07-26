let option = std.option

let option(comptime t: type) = enum {
  some(t),
  none,
}

let main(): i32 = { 42 }
