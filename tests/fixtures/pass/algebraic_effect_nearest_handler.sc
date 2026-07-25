let Read = effect {
  let read(): i32
}

let main(): i32 = {
  Read.handle read { (resume) -> resume(40) } action {
    let inner = Read.handle read { (resume) -> resume(2) } action {
      Read.read()
    }
    inner + Read.read()
  }
}

test("algebraic_effect_nearest_handler.sc") {
  main() == 42
}
