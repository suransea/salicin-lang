let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let invalid(T: type)(self: borrow(Self))(): T = { self.value }
}

let main(): i32 = { 0 }
