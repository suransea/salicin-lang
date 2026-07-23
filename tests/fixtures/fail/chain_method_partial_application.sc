use std.Option

let Number = struct { value: i32 }

extend Number {
  let plus(self: borrow(Self))(x: i32)(y: i32): i32 = { self.value + x + y }
}

let main(): i32 = {
  let add_last = Option(Number).Some(Number { value: 40 })?.plus(1)
  add_last(1) ?? 0
}
