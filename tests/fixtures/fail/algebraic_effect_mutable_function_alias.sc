let ask = effect {
  let value(): i32
}

let ask(): i32 with(ask) = {
  ask.value()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let mut action = ask
      action()
    }
}
