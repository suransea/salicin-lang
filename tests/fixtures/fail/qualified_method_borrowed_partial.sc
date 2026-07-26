let number = struct { value: i32 }

extend number {
  let add(self: borrow(self))(amount: i32): i32 = { self.value + amount }
}

let main(): i32 = {
  let number = number { value: 42 }
  let partial = number.add(number)
  0
}
