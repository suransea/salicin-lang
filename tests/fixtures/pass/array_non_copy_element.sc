let Boxed = struct { value: i32 }

let unwrap(move value: Boxed): i32 = { value.value }

let main(): i32 = {
  let mut values = [Boxed { value: 20 }, Boxed { value: 2 }]
  let first = unwrap(values[0])
  values[0] = Boxed { value: 40 }
  first + unwrap(values[0]) - unwrap(values[1]) - 16
}
