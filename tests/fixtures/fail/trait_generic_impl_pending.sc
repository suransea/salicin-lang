let Read = trait {
  let read(self: borrow(Self))(): i32
}

let Cell(T: type) = struct { value: T }

extend Cell: Read {
  let read(self: borrow(Self))(): i32 = { 0 }
}

let main(): i32 = { 0 }
