let option = core.option

let apply(
  move choose: (option(i32)): core.control.attempt(option(i32))(i32),
)(move input: option(i32)): core.control.attempt(option(i32))(i32) = {
  choose(input)
}

let main(): i32 = {
  let attempted = apply({ some(value) -> value })(option.some(42))
  match attempted
    { hit(value) -> value }
    { miss(_) -> 0 }
}

test("pattern_partial_pass.sc") {
  std.test.assert(main() == 42)
}
