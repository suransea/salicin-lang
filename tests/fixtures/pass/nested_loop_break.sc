let main(): i32 = {
  let mut answer = 40
  loop {
    loop {
      break()
    }
    answer = answer + 2
    break(answer)
  }
}

test("nested_loop_break.sc") {
  std.test.assert(main() == 42)
}
