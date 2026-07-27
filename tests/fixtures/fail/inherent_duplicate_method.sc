let a = struct { value: i32 }

extend(a) {
  let value_of(self: borrow(self))(): i32 = { self.value }
}

extend(a) {
  let value_of(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = { 0 }
