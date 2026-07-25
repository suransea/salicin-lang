let Option = std.Option

let apply(
  move choose: (Option(i32)): core.control.Attempt(Option(i32))(i32),
)(move input: Option(i32)): core.control.Attempt(Option(i32))(i32) = {
  choose(input)
}

let main(): i32 = {
  let attempted = apply({ Some(value) -> value })(Option.Some(42))
  match attempted
    { Hit(value) -> value }
    { Miss(_) -> 0 }
}

test("pattern_partial_pass.sc") {
  main() == 42
}
