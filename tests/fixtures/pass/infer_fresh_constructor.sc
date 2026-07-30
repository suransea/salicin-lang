let cell(comptime t: type) = struct { value: t }
let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = { identity(cell(i32) { value: 42 }).value }

test("infer_fresh_constructor.sc") {
  std.test.assert(main() == 42)
}
