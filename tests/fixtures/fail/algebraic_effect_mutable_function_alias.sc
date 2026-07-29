let ask = effect {
  let value(): i32
}

let ask: with(ask)(): i32 = {
  ask.value()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let mut action = ask
      action()
    }
}
