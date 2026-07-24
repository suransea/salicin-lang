let Ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let main(): i32 = {
  Ask.handle value { (input, resume) -> resume(input) } action {
    Ask.value(left: 42)
  }
}
