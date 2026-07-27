extend(number, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let read = trait {
  let read(self: borrow(self))(): i32
}

let number = struct { value: i32 }

let main(): i32 = {
  let number = number { value: 42 }
  number.read()
}

test("trait_declaration_order.sc") {
  main() == 42
}
