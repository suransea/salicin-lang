let ask = effect {
  let value(): i32
}

let left: with(ask)(): i32 = { ask.value() }
let right: with(ask)(): i32 = { ask.value() + 1 }

let leak: with(ask)(): (): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
      let selected: with(ask)((): i32)  = if true { left } else { right }
      selected
    }
}

let main(): i32 = { 0 }
