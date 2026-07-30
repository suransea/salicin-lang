let produce = trait {
  let item: type
  let produce(self: borrow(self))(): item
}

let value = struct { value: i32 }

extend(value, produce) {
  let item = i32
  let produce(self: borrow(self))(): i32 = { self.value }
}

let produce(comptime t: type)(value: borrow(t)): i32
= requires(t is produce && t.item == i32) { value.produce() }

let forward(comptime t: type)(value: borrow(t)): i32
= requires(t is produce && t.item == i32) { produce(value) }

let main(): i32 = {
  let value = value { value: 42 }
  forward(value)
}

test("where_associated_method.sc") {
  std.test.assert(main() == 42)
}
