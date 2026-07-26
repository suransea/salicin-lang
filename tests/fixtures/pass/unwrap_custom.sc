let unwrap_operator = std.flow.unwrap_operator

let present = enum {
  value(i32),
}

extend present: unwrap_operator {
  let output = i32

  let unwrap(move self): i32 = {
    match self
      { value(value) -> value }
  }
}

let main(): i32 = { present.value(42)!! }

test("unwrap_custom.sc") {
  main() == 42
}
