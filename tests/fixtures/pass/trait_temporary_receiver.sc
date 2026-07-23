let Read = trait {
  let read(self: borrow(Self))(): i32
}

let Number = struct { value: i32 }

extend Number: Read {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = { Number { value: 42 }.read() }
