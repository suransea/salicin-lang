let Cell(T: type) = struct { value: T }

let main(): i32 = {
  let flag = Cell(bool) { value: true }
  let answer = Cell(i32) { value: 42 }
  if flag.value { answer.value } else { 0 }
}
