let option = core.option

let number = struct { value: i32 }

extend(number) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = { option(number).some(number { value: 42 })?.read() ?? 0 }

test("chain_borrowed_method.sc") {
  main() == 42
}
