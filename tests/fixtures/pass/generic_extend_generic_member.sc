let cell(comptime t: type) = struct { value: t }

extend(cell(t)) {
  let identity(comptime u: type)(self: borrow(self))(move value: u): u = { value }
}

let main(): i32 = {
  let cell = cell { value: 0 }
  cell.identity(i32)(42)
}

test("generic_extend_generic_member.sc") {
  std.test.assert(main() == 42)
}
