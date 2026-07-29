let lend = trait {
  let item(comptime a: access)(comptime r: region): type
}

let require(comptime t: type)(move value: t): ()
= requires(t is lend && t.item(comptime a: access, comptime r: region) == borrow(a)(r)(i32)) {}

let main(): () = {}
