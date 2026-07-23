let Read = trait {
  let read(self: borrow(Self))(): i32
}

let Number = struct { value: i32 }

extend Number: Read {
}

let main(): i32 = { 0 }
