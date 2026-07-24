let Ask = effect {
  let value(): i32
}

let left(): i32 with(Ask) = {
  Ask.value()
}

let right(): i32 with(Ask) = {
  Ask.value() + 1
}

let fallback(): i32 with(Ask) = {
  Ask.value()
}

let invoke(action: (): i32 with(Ask)): i32 with(Ask) = {
  action()
}

let finish(value: i32): i32 with(Ask) = {
  value + 1
}

let select(mode: i32): i32 with(Ask) = {
  let action: (): i32 with(Ask) = if mode == 0 { left } else if mode == 1 { right } else { fallback }
  let direct = finish(action())
  let higher = invoke(action)
  direct + higher + 1
}

let main(): i32 = {
  Ask.handle value { (resume) -> resume(20) } action {
    select(2)
  }
}
