let Point = struct(raw: i32)

extend Point {
  let origin: Self = Self(40)
  let new(value: i32): Self = Self(value)
  let shifted(move self)(delta: i32): Self = Self(self.raw + delta)
  let read(borrow self)(): i32 = self.raw
  let read(value: Self): i32 = Self.read(self: value)()
}

let Choice = enum { Some(i32), None }

extend Choice {
  let unwrap(move self)(): i32 = self match {
    Self.Some(value) => value,
    Self.None => 0,
  }
}

let Rebuild = trait {
  let rebuild(move self)(): Self
  let read(borrow self)(): i32
  let twice(borrow self)(): i32 = Self.read(self: self)() + Self.read(self: self)()
}

let Wrapper = struct(raw: i32)

extend Wrapper: Rebuild {
  let rebuild(move self)(): Self = Self(self.raw)
  let read(borrow self)(): i32 = self.raw
}

let main(): i32 = {
  let shifted = Point.new(40).shifted(2)
  let point = Point.read(shifted)
  let choice = Choice.Some(42).unwrap()
  let wrapper = Wrapper(42).rebuild().raw
  let default = Wrapper(21).twice()
  let origin = Point.origin.raw
  point + choice + wrapper + default + origin - 166
}
