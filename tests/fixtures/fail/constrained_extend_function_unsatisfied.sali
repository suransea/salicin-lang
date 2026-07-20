let Resource = struct(value: i32)
let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T)
where T: Copy {
  let new(copy value: T): Cell(T) = Cell(value)
}

let main(): i32 = Cell.new(Resource(42)).value.value
