let Check = trait {
  let check(borrow self)(): i32
}

let Number = struct(value: i32)

extend Number: Check {
  let check(borrow self)(): bool = { true }
}

let main(): i32 = { 0 }
