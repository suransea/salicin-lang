let produce = trait {
  let item: type
  let produce(self: borrow(self))(): item
}

let value = struct { value: i32 }

extend(value, produce) {
  let item = i32
  let produce(self: borrow(self))(): i32 = { self.value }
}

let require_bool(comptime t: type)(value: borrow(t)): bool
= requires(t is produce && t.item == bool) { value.produce() }

let main(): i32 = {
  let value = value { value: 42 }
  if require_bool(value) { 42 } else { 0 }
}
