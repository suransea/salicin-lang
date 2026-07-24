let Result = std.Result

let Result(E: type)(T: type) = enum {
  Ok(T),
  Err(E),
}

let main(): i32 = { 42 }
