let read = trait {
  let read(self: borrow(self))(): i32
}

let cell(comptime t: type) = struct { value: t }

extend cell(i32): read {
  let read(self: borrow(self))(): i32 = { self.value }
}

extend(comptime t: type) cell(t) {
  let take(move self)(): t = { self.value }
}

let main(): i32 = {
  let cell_value = cell(i32) { value: 42 }
  let read = cell.read(cell_value)()
  let taken = cell(i32).take(cell_value)()
  read + taken - 42
}

test("qualified_trait_generic_method.sc") {
  main() == 42
}
