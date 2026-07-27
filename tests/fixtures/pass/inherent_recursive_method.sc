let number = struct { value: i32 }

extend(number) {
  let descend(self: borrow(self))(remaining: i32): i32 = {
    if remaining == 0 {
      self.value
    } else {
      self.descend(remaining - 1)
    }
  }
}

let main(): i32 = {
  let number = number { value: 42 }
  number.descend(3)
}

test("inherent_recursive_method.sc") {
  main() == 42
}
