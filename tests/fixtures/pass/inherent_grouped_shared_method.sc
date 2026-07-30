let number = struct { value: i32 }

extend(number) {
  let plus(self: borrow(self))(x: i32)(y: i32): i32 = { self.value + x + y }
}

let main(): i32 = {
  let number = number { value: 40 }
  number.plus(1)(1)
}

test("inherent_grouped_shared_method.sc") {
  std.test.assert(main() == 42)
}
