let cell = struct { value: i32 }

extend(cell) {
  let get(comptime r: region)(self: borrow(r)(self))(): borrow(r)(i32) = { borrow(self.value) }
}

let main(): i32 = {
  let mut cell = cell { value: 42 }
  let reference = cell.get()
  cell.value = 0
  reference
}
