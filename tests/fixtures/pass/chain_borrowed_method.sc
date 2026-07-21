let Number = struct(value: i32)

extend Number {
  let read(borrow self)(): i32 = { self.value }
}

let main(): i32 = { Option(Number).Some(Number(42))?.read() ?? 0 }
