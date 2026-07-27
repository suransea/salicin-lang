let read = trait {
  let read(self: borrow(self))(): i32
}

let cell(comptime t: type) = struct { value: t }

extend(cell, read) {
  let read(self: borrow(self))(): i32 = { 0 }
}

let main(): i32 = { 0 }
