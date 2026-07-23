let Measure = trait {
  let measure(self: borrow(Self))(): i32
}

let read(T: type)(value: borrow(T)): i32 = { value.measure() }

let main(): i32 = { 0 }
