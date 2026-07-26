let number = struct { value: i32 }

let main(): i32 = {
  let answer = number { value: 40 } + number { value: 2 }
  answer.value
}
