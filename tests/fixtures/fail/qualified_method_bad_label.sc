let Number = struct { value: i32 }

extend Number {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = {
  let number = Number { value: 42 }
  Number.read(receiver: number)()
}
