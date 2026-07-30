let cell = struct { value: i32 }
let read(comptime t: type)(value: borrow(t)): i32 = { 42 }

let main(): i32 = { read(cell { value: 42 }) }

test("infer_borrow_temporary.sc") {
  std.test.assert(main() == 42)
}
