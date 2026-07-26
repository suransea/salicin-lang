let a = struct { reset: i32 }

extend a {
  let reset(self: borrow(self))(): i32 = { self.reset }
}

let main(): i32 = { 0 }
