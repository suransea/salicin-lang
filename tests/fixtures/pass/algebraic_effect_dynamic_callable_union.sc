let Ask = effect {
  let choose(): bool
  let value(): i32
}

let left(): i32 with(Ask) = { Ask.value() + 1 }
let middle(): i32 with(Ask) = { Ask.value() + 2 }
let right(): i32 with(Ask) = { Ask.value() + 3 }

let main(): i32 = {
  Ask.handle choose { (resume) -> resume(false) } value { (resume) -> resume(39) } action {
    let first: (): i32 with(Ask) = if true { left } else { middle }
    let second: (): i32 with(Ask) = if false { middle } else { right }
    let combined: (): i32 with(Ask) = if Ask.choose() { first } else { second }
    combined()
  }
}
