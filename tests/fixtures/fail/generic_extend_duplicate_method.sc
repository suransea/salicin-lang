let cell(comptime t: type) = struct { value: t }

extend(cell(t)) {
  let answer(self: borrow(self))(): i32 = { 1 }
}

extend(cell(t)) {
  let answer(self: borrow(self))(): i32 = { 2 }
}

let main(): i32 = {
  let cell = cell { value: 0 }
  cell.answer()
}
