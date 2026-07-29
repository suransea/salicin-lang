let ask = effect {
  let value(): i32
}

let apply_twice: with(ask)(move action: with(ask)((): i32)): i32 = {
  action() + action()
}

let run(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) -> resume(21) } action {
      apply_twice(action)
    }
}

let main(): i32 = {
  let action: with(ask)((): i32)  = { () ->
    ask.value()
  }
  run(action)
}
