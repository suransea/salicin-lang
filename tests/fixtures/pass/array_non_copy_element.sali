let Boxed = struct(value: i32)

let unwrap(move value: Boxed): i32 = value.value

let main(): i32 = {
  let mut values = [Boxed(20), Boxed(2)]
  let first = unwrap(values[0])
  values[0] = Boxed(40)
  first + unwrap(values[0]) - unwrap(values[1]) - 16
}
