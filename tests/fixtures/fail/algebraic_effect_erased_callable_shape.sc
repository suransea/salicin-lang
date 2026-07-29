let ask = effect {
  let value(): i32
}

let apply: with(ask)(action: with(ask)((): i32)): i32 = {
  action()
}

let run(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      apply(action)
    }
}

let main(): i32 = {
  let action: with(ask)((): i32)  = { () ->
    ask.value()
  }
  run(action)
}
