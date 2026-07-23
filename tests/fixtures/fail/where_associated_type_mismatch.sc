let Produce = trait {
  let Item: type
  let produce(self: borrow(Self))(): Item
}

let Value = struct { value: i32 }

extend Value: Produce {
  let Item = i32
  let produce(self: borrow(Self))(): i32 = { self.value }
}

let require_bool(T: type)(value: borrow(T)): bool
where T: Produce(Item = bool) = { value.produce() }

let main(): i32 = {
  let value = Value { value: 42 }
  if require_bool(value) { 42 } else { 0 }
}
