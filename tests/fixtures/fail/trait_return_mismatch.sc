let check = trait {
  let check(self: borrow(self))(): i32
}

let number = struct { value: i32 }

extend(number, check) {
  let check(self: borrow(self))(): bool = { true }
}

let main(): i32 = { 0 }
