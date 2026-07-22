let Number = struct { value: i32 }

extend Number {
  let read(borrow self)(): i32 = { self.value }
}

let main(): i32 = { Number.read()() }
