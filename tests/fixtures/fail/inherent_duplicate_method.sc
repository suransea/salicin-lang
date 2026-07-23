let A = struct { value: i32 }

extend A {
  let value_of(self: borrow(Self))(): i32 = { self.value }
}

extend A {
  let value_of(self: borrow(Self))(): i32 = { self.value }
}

let main(): i32 = { 0 }
