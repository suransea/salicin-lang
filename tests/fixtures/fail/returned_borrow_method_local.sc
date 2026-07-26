let cell = struct { value: i32 }

extend cell {
  let get(comptime r: region)(self: borrow(r)(self))(): borrow(r)(i32) = { borrow(self.value) }
}

let bad(comptime r: region)(seed: borrow(r)(i32)): borrow(r)(i32) = {
  let cell = cell { value: seed }
  cell.get()
}

let main(): i32 = { 42 }
