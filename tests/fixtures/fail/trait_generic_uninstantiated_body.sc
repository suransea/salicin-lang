let Read = trait {
  let read(self: borrow(Self))(): i32
}

let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T): Read {
  let read(self: borrow(Self))(): i32 = { missing }
}

let main(): i32 = { 42 }
