let number = struct { value: i32 }

extend(number) {
  let add(self: borrow(self))(other: borrow(number)): i32 = { self.value + other.value }
}

let main(): i32 = { number { value: 20 }.add(number { value: 22 }) }

test("temporary_borrow_method_argument.sc") {
  main() == 42
}
