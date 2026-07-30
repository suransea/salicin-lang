let probe = effect {
  let read(): bool
}

let main(): i32 = {
  probe.handle read { (resume) -> resume(true) } done {
      (value) -> if value { 42 } else { 0 }
    } action {
      probe.read()
    }
}

test("algebraic_effect_done.sc") {
  std.test.assert(main() == 42)
}
