let read = trait {
  let read(self: borrow(self))(): i32
}

let number = struct { value: i32 }

extend(number, read) {
  let read(self: borrow(self))(): i32 = { self.value }
  let extra(self: borrow(self))(): i32 = { 0 }
}

let main(): i32 = { 0 }
