let option = core.option

let adder = struct { base: i32 }

extend(adder) {
  let add(self)(value: i32): i32 = { self.base + value }
}

let side_effect(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  1
}

let main(): i32 = {
  let mut count = 0
  let answer = option(adder).none?.add(side_effect(count)) ?? 42
  if count == 0 { answer } else { 0 }
}

test("chain_option_method_arguments_are_lazy.sc") {
  main() == 42
}
