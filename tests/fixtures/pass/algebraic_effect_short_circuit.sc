let Stop = effect {
  let stop(): bool
}

let main(): i32 = {
  Stop.handle stop { (resume) -> 1 } action {
    let skipped = false && Stop.stop()
    if skipped { 0 } else { 42 }
  }
}
