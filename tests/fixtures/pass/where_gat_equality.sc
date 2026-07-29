let identity(comptime t: type): type = t

let factory = trait {
  let item(comptime t: type): type

  let make(self: borrow(self))(value: i32): item(i32)
}

let cell = struct {}

extend(cell, factory) {
  let item = identity

  let make(self: borrow(self))(value: i32): i32 = { value }
}

let make_i32(comptime t: type)(value: borrow(t)): i32
= requires(t is factory && t.item(comptime u: type) == u) {
  value.make(42)
}

let main(): i32 = {
  let cell = cell {}
  make_i32(cell)
}

test("where_gat_equality.sc") {
  main() == 42
}
