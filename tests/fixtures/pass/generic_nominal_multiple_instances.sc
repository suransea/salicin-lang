let Cell(T: type) = struct(value: T)

let main(): i32 = {
  let flag = Cell(bool)(true)
  let answer = Cell(i32)(42)
  if flag.value { answer.value } else { 0 }
}
