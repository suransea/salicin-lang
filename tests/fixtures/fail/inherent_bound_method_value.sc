let Number = struct { raw: i32 }

extend Number {
  let value(borrow self)(): i32 = { self.raw }
}

let main(): i32 = {
  let number = Number { value: 42 }
  let bound = number.value
  bound()
}
