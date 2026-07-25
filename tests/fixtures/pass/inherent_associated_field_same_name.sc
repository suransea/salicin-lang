let Answer = struct { answer: i32 }

extend Answer {
  let answer = 2
}

let main(): i32 = {
  let value = Answer { answer: 40 }
  value.answer + Answer.answer
}

test("inherent_associated_field_same_name.sc") {
  main() == 42
}
