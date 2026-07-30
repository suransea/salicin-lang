let unwrap = core.flow.unwrap

let present = enum {
  value(i32),
}

extend(present, unwrap) {
  let output = i32

  let unwrap(move self): i32 = {
    match self
      { value(value) -> value }
  }
}

let main(): i32 = { present.value(42)!! }

test("unwrap_custom.sc") {
  std.test.assert(main() == 42)
}
