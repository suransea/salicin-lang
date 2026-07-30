let answer = struct { answer: i32 }

extend(answer) {
  let answer = 2
}

let main(): i32 = {
  let answer = answer { answer: 40 }
  answer.answer + 2
}

test("inherent_local_shadows_type.sc") {
  std.test.assert(main() == 42)
}
