let Cell(T: type) = struct { value: T }

extend(T: type, U: type) Cell(T) {
  let invalid(self: borrow(Self))(): i32 = { 0 }
}

let main(): i32 = { 0 }
