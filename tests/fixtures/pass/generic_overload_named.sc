let choose(comptime t: type)(left: t): t = { left }
let choose(comptime t: type)(right: t): t = { right }

let counter = struct { value: i32 }

extend counter {
  let add(comptime t: type)(self: borrow(self))(left: t): t = { left }
  let add(comptime t: type)(self: borrow(self))(right: t): t = { right }
}

let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t) {
  let choose(left: t): t = { left }
  let choose(right: t): t = { right }
  let add(self: borrow(self))(left: t): t = { left }
  let add(self: borrow(self))(right: t): t = { right }
}

let main(): i32 = {
  choose(left: 10) + cell.choose(right: 10) + cell(i32) { value: 0 }.add(left: 22)
}

test("generic_overload_named.sc") {
  main() == 42
}
