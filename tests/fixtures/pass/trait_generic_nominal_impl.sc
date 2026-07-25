let Read = trait {
  let read(self: borrow(Self))(): i32
}

let Cell(T: type) = struct { value: T }

extend Cell(i32): Read {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = {
  let cell = Cell(i32) { value: 42 }
  cell.read()
}

test("trait_generic_nominal_impl.sc") {
  main() == 42
}
