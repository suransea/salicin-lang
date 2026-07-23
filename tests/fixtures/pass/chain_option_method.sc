use std.Option

let Number = struct { value: i32 }

extend Number {
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = { Option(Number).Some(Number { value: 42 })?.take() ?? 0 }
