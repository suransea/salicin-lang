let resource = struct { value: i32 }
let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t)
where t: copyable {
  let duplicate(self: borrow(self))(): t = { self.value }
}

let main(): i32 = {
  let cell = cell { value: resource { value: 42 } }
  cell.duplicate().value
}
