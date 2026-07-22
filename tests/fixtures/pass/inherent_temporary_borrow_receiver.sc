let Number = struct { raw: i32 }

extend Number {
  let value(borrow self)(): i32 = { self.raw }
}

let main(): i32 = { Number { raw: 42 }.value() }
