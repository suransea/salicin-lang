let result = core.result

let boxed = struct { answer: bool }

let main(): i32 = {
  let answer = result(bool)(boxed).ok(boxed { answer: true })?.answer
  if answer ?? false { 42 } else { 0 }
}

test("chain_success_type_changes.sc") {
  main() == 42
}
