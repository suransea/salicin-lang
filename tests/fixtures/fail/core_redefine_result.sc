let result = std.result

let result(e: type)(comptime t: type) = enum {
  ok(t),
  err(e),
}

let main(): i32 = { 42 }
