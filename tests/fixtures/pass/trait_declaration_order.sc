extend Number: Read {
  let read(borrow self)(): i32 = { self.value }
}

let Read = trait {
  let read(borrow self)(): i32
}

let Number = struct(value: i32)

let main(): i32 = {
  let number = Number(42)
  number.read()
}
