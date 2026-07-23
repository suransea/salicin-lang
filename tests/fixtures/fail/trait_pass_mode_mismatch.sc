let Update = trait {
  let update(self: borrow(Self))(value: borrow(i32)): i32
}

let Number = struct { value: i32 }

extend Number: Update {
  let update(self: borrow(Self))(copy value: i32): i32 = { self.value }
}

let main(): i32 = { 0 }
