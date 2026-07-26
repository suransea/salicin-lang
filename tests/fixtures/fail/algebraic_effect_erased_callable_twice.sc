let ask = effect {
  let value(): i32
}

let apply_twice(move action: (): i32 with(ask)): i32 with(ask) = {
  action() + action()
}

let run(move action: (): i32 with(ask)): i32 = {
  ask.handle value { (resume) -> resume(21) } action {
      apply_twice(action)
    }
}

let main(): i32 = {
  let action: (): i32 with(ask) = { () ->
    ask.value()
  }
  run(action)
}
