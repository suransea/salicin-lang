let Ask = effect {
  let value(): i32
}

let left(): i32 with(Ask) = { Ask.value() }
let right(): i32 with(Ask) = { Ask.value() + 1 }

let leak(): (): i32 with(Ask) = {
  Ask.handle value { (resume) -> resume(42) } action {
    let selected: (): i32 with(Ask) = if true { left } else { right }
    selected
  }
}

let main(): i32 = { 0 }
