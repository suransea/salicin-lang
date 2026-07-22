let Number = struct { value: i32 }

extend Number {
  let descend(borrow self)(remaining: i32): i32 = {
    if remaining == 0 {
      self.value
    } else {
      self.descend(remaining - 1)
    }
  }
}

let main(): i32 = {
  let number = Number { value: 42 }
  number.descend(3)
}
