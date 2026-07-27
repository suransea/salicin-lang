let answer = struct { answer: i32 }

extend(answer) {
  let answer = 2
}

let main(): i32 = {
  let value = answer { answer: 40 }
  value.answer + answer.answer
}

test("inherent_associated_field_same_name.sc") {
  main() == 42
}
