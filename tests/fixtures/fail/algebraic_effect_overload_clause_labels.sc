let ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let main(): i32 = {
  ask.handle value { (input, resume) -> resume(input) } action {
      ask.value(left: 42)
    }
}
