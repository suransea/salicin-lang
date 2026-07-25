let Answer = struct { answer: i32 }

extend Answer {
  let answer = 2
}

let main(): i32 = {
  let Answer = Answer { answer: 40 }
  Answer.answer + 2
}

test("inherent_local_shadows_type.sc") {
  main() == 42
}
