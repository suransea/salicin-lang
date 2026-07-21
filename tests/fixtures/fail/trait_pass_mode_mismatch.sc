let Update = trait {
  let update(borrow self)(borrow value: i32): i32
}

let Number = struct(value: i32)

extend Number: Update {
  let update(borrow self)(copy value: i32): i32 = self.value
}

let main(): i32 = 0
