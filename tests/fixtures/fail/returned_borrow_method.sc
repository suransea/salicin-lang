let cell = struct { value: i32 }

extend(cell) {
  let get(comptime r: region)(self: borrow(r)(self))(): borrow(r)(i32) = { borrow(self.value) }
}

let main(): i32 = {
  let reference = cell { value: 42 }.get()
  reference
}
