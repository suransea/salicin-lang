let read = trait {
  let read(self: borrow(self))(): i32
  let doubled(self: borrow(self))(): i32 = { self.read() + self.read() }
}

let number = struct { value: i32 }

extend number: read {
  let read(self: borrow(self))(): i32 = { self.value }
}

let override = struct {}

extend override: read {
  let read(self: borrow(self))(): i32 = { 0 }
  let doubled(self: borrow(self))(): i32 = { 42 }
}

let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t): read
where t: read {
  let read(self: borrow(self))(): i32 = { self.value.read() }
}

let take = trait {
  let item: type
  let take(move self)(): item
  let forward(move self)(): item = { self.take() }
}

let boxed = struct { value: i32 }

extend boxed: take {
  let item = i32
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let number = number { value: 21 }
  let cell = cell { value: number }
  let overridden = override {}
  let boxed = boxed { value: 42 }
  cell.doubled() + overridden.doubled() + boxed.forward() - 84
}

test("trait_default_method.sc") {
  main() == 42
}
