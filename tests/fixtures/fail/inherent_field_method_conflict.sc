let A = struct { reset: i32 }

extend A {
  let reset(self: borrow(Self))(): i32 = { self.reset }
}

let main(): i32 = { 0 }
