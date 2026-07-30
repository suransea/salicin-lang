let measure = trait {
  let measure(self: borrow(self))(): i32
}

let value = struct { value: i32 }

extend(value, measure) {
  let measure(self: borrow(self))(): i32 = { self.value }
}

let read(comptime t: type)(value: borrow(t)): i32
= requires(t is measure) { value.measure() }

let forward(comptime t: type)(value: borrow(t)): i32
= requires(t is measure) { read(value) }

let main(): i32 = {
  let value = value { value: 42 }
  forward(value)
}

test("where_method_dispatch.sc") {
  std.test.assert(main() == 42)
}
