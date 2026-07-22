let A = struct { reset: i32 }

extend A {
  let reset(borrow self)(): i32 = { self.reset }
}

let main(): i32 = { 0 }
