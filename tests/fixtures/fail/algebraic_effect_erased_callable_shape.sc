let ask = effect {
  let value(): i32
}

let apply(action: (): i32 with(ask)): i32 with(ask) = {
  action()
}

let run(move action: (): i32 with(ask)): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      apply(action)
    }
}

let main(): i32 = {
  let action: (): i32 with(ask) = { () ->
    ask.value()
  }
  run(action)
}
