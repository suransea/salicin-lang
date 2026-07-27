let read = trait {
  let read(self: borrow(self))(): i32
}

let cell(comptime t: type) = struct { value: t }

extend(cell(i32), read) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = {
  let cell = cell(i32) { value: 42 }
  cell.read()
}

test("trait_generic_nominal_impl.sc") {
  main() == 42
}
