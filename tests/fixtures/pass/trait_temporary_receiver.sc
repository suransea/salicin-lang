let read = trait {
  let read(self: borrow(self))(): i32
}

let number = struct { value: i32 }

extend(number, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = { number { value: 42 }.read() }

test("trait_temporary_receiver.sc") {
  main() == 42
}
