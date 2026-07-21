let Marker = trait {}
let Value = struct(value: i32)
extend Value: Copy {}
extend Value: Marker {}

let duplicate(T: type)(copy value: T): T
where T: Copy,
      T: Marker, = {
  let first = value
  value
}

let main(): i32 = duplicate(Value(42)).value
