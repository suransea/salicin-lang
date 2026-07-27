let option = core.option

let boxed = struct { value: i32 }

let make(count: borrow(mut)(i32)): option(boxed) = {
  count = count + 1
  option(boxed).some(boxed { value: 42 })
}

let main(): i32 = {
  let mut count = 0
  let answer = make(count)?.value ?? 0
  if count == 1 { answer } else { 0 }
}

test("chain_lhs_once.sc") {
  main() == 42
}
