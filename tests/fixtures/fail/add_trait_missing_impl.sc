let Number = struct { value: i32 }

let main(): i32 = {
  let answer = Number { value: 40 } + Number { value: 2 }
  answer.value
}
