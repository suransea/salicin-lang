let Resource = struct(value: i32)
let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T)
where T: Copy {
  let duplicate(borrow self)(): T = { self.value }
}

let main(): i32 = {
  let cell = Cell(Resource(42))
  cell.duplicate().value
}
