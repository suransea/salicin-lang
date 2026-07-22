let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }}

let Cell (T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let new(move value: T): Cell(T) = { Cell { value: value } }
  let take(move self)(): T = { self.value }
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let cell = Cell.new(Resource { counter: counter })
    let resource = cell.take()
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  41 + drops
}
