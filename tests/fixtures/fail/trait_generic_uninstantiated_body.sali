let Read = trait {
  let read(borrow self)(): i32
}

let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T): Read {
  let read(borrow self)(): i32 = missing
}

let main(): i32 = 42
