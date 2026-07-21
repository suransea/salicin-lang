let Measure = trait {
  let measure(borrow self)(): i32
}

let read(T: type)(borrow value: T): i32 = { value.measure() }

let main(): i32 = { 0 }
