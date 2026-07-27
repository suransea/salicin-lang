let cell(comptime t: type) = struct { value: t }

extend(cell(i32)) {
  let invalid(self: borrow(self))(): i32 = { 0 }
}

let main(): i32 = { 0 }
