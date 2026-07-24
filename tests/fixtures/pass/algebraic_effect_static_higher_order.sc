let Ask = effect {
  let value(): i32
}

let ask(): i32 with(Ask) = {
  Ask.value()
}

let invoke(action: (): i32 with(Ask)): i32 with(Ask) = {
  action()
}

let main(): i32 = {
  Ask.handle value { (resume) -> resume(42) } action {
    let selected = ask
    invoke(selected)
  }
}
