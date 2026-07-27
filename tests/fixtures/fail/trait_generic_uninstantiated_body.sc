let read = trait {
  let read(self: borrow(self))(): i32
}

let cell(comptime t: type) = struct { value: t }

extend(cell(t), read) {
  let read(self: borrow(self))(): i32 = { missing }
}

let main(): i32 = { 42 }
