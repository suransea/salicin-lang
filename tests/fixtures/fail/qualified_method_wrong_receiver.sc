let Number = struct { value: i32 }
let Other = struct { value: i32 }

extend Number {
  let read(self: borrow(Self))(): i32 = { self.value }
}

extend Other {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = { Number.read(Other { value: 42 })() }
