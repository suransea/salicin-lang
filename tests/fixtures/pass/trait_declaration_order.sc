extend Number: Read {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let Read = trait {
  let read(self: borrow(Self))(): i32
}

let Number = struct { value: i32 }

let main(): i32 = {
  let number = Number { value: 42 }
  number.read()
}
