let tick = effect {
  let tick(): i32
}

let main(): i32 = {
  let mut count = 0
  tick.handle tick { (resume) -> resume(1) } action {
      while { count + tick.tick() <= 2 } {
        count += 1
        if count == 1 { continue() }
      }
      let stopped = loop {
        count += tick.tick()
        if count == 3 { break(count) }
      }
      36 + count + stopped
    }
}

test("algebraic_effect_loops.sc") {
  std.test.assert(main() == 42)
}
