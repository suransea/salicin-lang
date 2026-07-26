let number = struct { raw: i32 }

extend number {
  let value(self: borrow(self))(): i32 = { self.raw }
}

let main(): i32 = { number { raw: 42 }.value() }

test("inherent_temporary_borrow_receiver.sc") {
  main() == 42
}
