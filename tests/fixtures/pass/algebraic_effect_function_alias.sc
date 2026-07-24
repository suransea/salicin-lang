let Ask = effect {
  let value(): i32
}

let ask(): i32 with(Ask) = {
  Ask.value()
}

let main(): i32 = {
  Ask.handle value { (resume) -> resume(42) } action {
    let action = ask
    let forwarded = action
    forwarded()
  }
}
