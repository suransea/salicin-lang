let same(comptime t: type)(left: t, right: t): t = { left }

let main(): i32 = {
  same(1, true)
  42
}
