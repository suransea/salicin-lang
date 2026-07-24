let Unwrap = std.flow.Unwrap

let Present = enum {
  Value(i32),
}

extend Present: Unwrap {
  let Output = i32

  let unwrap(move self): i32 = {
    match self
      { Value(value) -> value }
  }
}

let main(): i32 = { Present.Value(42)!! }
