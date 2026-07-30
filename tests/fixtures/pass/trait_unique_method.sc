let add_value = trait {
  let add(self: borrow(self))(value: i32): i32
}

let number = struct { value: i32 }

extend(number, add_value) {
  let add(self: borrow(self))(value: i32): i32 = { self.value + value }
}

let main(): i32 = {
  let number = number { value: 40 }
  number.add(2)
}

test("trait_unique_method.sc") {
  std.test.assert(main() == 42)
}
