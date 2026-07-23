use std.Option

let Number = struct { value: i32 }

extend Number {
  let read(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = { Option(Number).Some(Number { value: 42 })?.read() ?? 0 }
