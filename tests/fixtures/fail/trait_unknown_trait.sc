let number = struct { value: i32 }

extend number: missing_trait {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = { 0 }
