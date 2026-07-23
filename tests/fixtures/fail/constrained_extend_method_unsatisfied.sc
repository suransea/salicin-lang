let Resource = struct { value: i32 }
let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T)
where T: Copy {
  let duplicate(self: borrow(Self))(): T = { self.value }
}

let main(): i32 = {
  let cell = Cell { value: Resource { value: 42 } }
  cell.duplicate().value
}
