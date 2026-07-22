let Number = struct { value: i32 }

extend Number {
  let add(borrow self)(amount: i32): i32 = { self.value + amount }
}

let main(): i32 = {
  let number = Number { value: 42 }
  let partial = Number.add(number)
  0
}
