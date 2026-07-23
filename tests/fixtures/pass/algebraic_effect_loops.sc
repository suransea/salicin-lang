let Tick = effect {
  let tick(): i32
}

let main(): i32 = {
  let mut count = 0
  Tick.handle(
    tick: { (resume) -> resume(1) },
  ) {
    while { count + Tick.tick() <= 2 } {
      count += 1
      if count == 1 { continue }
    }
    let stopped = loop {
      count += Tick.tick()
      if count == 3 { break count }
    }
    36 + count + stopped
  }
}
