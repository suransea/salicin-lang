let number = struct { value: i32 }

extend number {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = {
  let value = number { value: 42 }
  number.read(receiver: value)()
}
