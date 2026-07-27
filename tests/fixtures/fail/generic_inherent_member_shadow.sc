let cell(comptime t: type) = struct { value: t }

extend(cell(t)) {
  let invalid(comptime t: type)(self: borrow(self))(): t = { self.value }
}

let main(): i32 = { 0 }
