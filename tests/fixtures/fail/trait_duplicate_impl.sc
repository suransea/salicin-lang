let Read = trait {
  let read(borrow self)(): i32
}

let Number = struct { value: i32 }

extend Number: Read {
  let read(borrow self)(): i32 = { self.value }
}

extend Number: Read {
  let read(borrow self)(): i32 = { self.value }
}

let main(): i32 = { 0 }
