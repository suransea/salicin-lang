let inner = enum {
  value( value: i32 ),
  empty,
}

let outer = enum {
  wrapped(inner),
  empty,
}

let read(value: outer): i32 = { match value
    { outer.wrapped(inner) -> match inner
      { inner.value( value: number ) -> number }
      { inner.empty -> 0 } }
    { outer.empty -> 0 }
}

let main(): i32 = { read(outer.wrapped(inner.value( value: 42 ))) }

test("nested_match.sc") {
  main() == 42
}
