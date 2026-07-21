let Probe = effect {
  let read(): bool
}

let main(): i32 = {
  Probe.handle(
    read: { (resume) -> resume(true) },
    done: { (value) -> if value { 42 } else { 0 } },
  ) {
    Probe.read()
  }
}
