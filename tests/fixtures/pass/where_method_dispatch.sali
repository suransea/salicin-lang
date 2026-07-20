let Measure = trait {
  let measure(borrow self)(): i32
}

let Value = struct(value: i32)

extend Value: Measure {
  let measure(borrow self)(): i32 = self.value
}

let read(T: type)(borrow value: T): i32
where T: Measure = value.measure()

let forward(T: type)(borrow value: T): i32
where T: Measure = read(value)

let main(): i32 = {
  let value = Value(42)
  forward(value)
}
