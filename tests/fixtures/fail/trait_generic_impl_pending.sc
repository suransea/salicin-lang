let Read = trait {
  let read(borrow self)(): i32
}

let Cell(T: type) = struct { value: T }

extend Cell: Read {
  let read(borrow self)(): i32 = { 0 }
}

let main(): i32 = { 0 }
