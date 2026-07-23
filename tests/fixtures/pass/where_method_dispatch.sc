let Measure = trait {
  let measure(self: borrow(Self))(): i32
}

let Value = struct { value: i32 }

extend Value: Measure {
  let measure(self: borrow(Self))(): i32 = { self.value }
}

let read(T: type)(value: borrow(T)): i32
where T: Measure = { value.measure() }

let forward(T: type)(value: borrow(T)): i32
where T: Measure = { read(value) }

let main(): i32 = {
  let value = Value { value: 42 }
  forward(value)
}
