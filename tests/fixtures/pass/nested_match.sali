let Inner = enum {
  Value(i32),
  Empty,
}

let Outer = enum {
  Wrapped(Inner),
  Empty,
}

let read(value: Outer): i32 = value match {
  Outer.Wrapped(inner) => inner match {
    Inner.Value(number) => number,
    Inner.Empty => 0,
  },
  Outer.Empty => 0,
}

let main(): i32 = read(Outer.Wrapped(Inner.Value(42)))
