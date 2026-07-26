let ask = effect {
  let value(): i32
}

let left(): i32 with(ask) = { ask.value() }
let right(): i32 with(ask) = { ask.value() + 1 }

let leak(): (): i32 with(ask) = {
  ask.handle value { (resume) -> resume(42) } action {
      let selected: (): i32 with(ask) = if true { left } else { right }
      selected
    }
}

let main(): i32 = { 0 }
