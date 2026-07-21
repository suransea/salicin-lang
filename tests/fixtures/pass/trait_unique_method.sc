let AddValue = trait {
  let add(borrow self)(value: i32): i32
}

let Number = struct(value: i32)

extend Number: AddValue {
  let add(borrow self)(value: i32): i32 = { self.value + value }
}

let main(): i32 = {
  let number = Number(40)
  number.add(2)
}
