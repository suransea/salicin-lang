let Ask = effect {
  let value(): i32
}

let left(): i32 with(Ask) = { Ask.value() }
let right(): i32 with(Ask) = { Ask.value() + 1 }

let main(): i32 = {
  Ask.handle value { (resume) -> resume(40) } action {
    let first: (): i32 with(Ask) = if true { left } else { right }
    let second: (): i32 with(Ask) = if true { right } else { left }
    let mut selected = first
    selected = second
    selected() + 1
  }
}
