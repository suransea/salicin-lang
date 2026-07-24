let Ask = effect {
  let value(): i32
}

let run()(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    action()
  }
}

let main(): i32 = {
  let mut base = 31
  run() { () ->
    base = base + 1
    Ask.value() + base
  }
}
