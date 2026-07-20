let Resource = struct(value: i32)

let duplicate(T: type)(copy value: T): T
where T: Copy = {
  let first = value
  value
}

let main(): i32 = duplicate(Resource(42)).value
