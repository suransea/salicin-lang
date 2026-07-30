let option = core.option

let number = struct { value: i32 }

extend(number) {
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = { option(number).some(number { value: 42 })?.take() ?? 0 }

test("chain_option_method.sc") {
  std.test.assert(main() == 42)
}
