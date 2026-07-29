let ask = effect {
  let value(): i32
}

let ask: with(ask)(): i32 = {
  ask.value()
}

let leak: with(ask)(): (): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let action = ask
      action
    }
}

let main(): i32 = { 0 }
