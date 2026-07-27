let number = struct { raw: i32 }

extend(number) {
  let value(self: borrow(self))(): i32 = { self.raw }
  let value = 2
}

let main(): i32 = {
  let number_value = number { raw: 40 }
  number_value.value() + number.value
}

test("inherent_method_and_associated_same_name.sc") {
  main() == 42
}
