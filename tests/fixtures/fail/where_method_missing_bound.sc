let measure = trait {
  let measure(self: borrow(self))(): i32
}

let read(comptime t: type)(value: borrow(t)): i32 = { value.measure() }

let main(): i32 = { 0 }
