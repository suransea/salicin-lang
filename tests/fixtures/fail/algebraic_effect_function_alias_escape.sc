let ask = effect {
  let value(): i32
}

let ask(): i32 with(ask) = {
  ask.value()
}

let leak(): (): i32 with(ask) = {
  ask.handle value { (resume) -> resume(42) } action {
      let action = ask
      action
    }
}

let main(): i32 = { 0 }
