let Check = trait {
  let check(self: borrow(Self))(): i32
}

let Number = struct { value: i32 }

extend Number: Check {
  let check(self: borrow(Self))(): bool = { true }
}

let main(): i32 = { 0 }
