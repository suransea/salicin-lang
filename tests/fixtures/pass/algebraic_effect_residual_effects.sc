use core.effects.Unsafe

let Supply = effect {
  let seed(): i32
}

let Ask = effect {
  let value(): i32 with(Supply, throws(bool), Unsafe)
}

let request(): i32 with(Ask, Supply, throws(bool), Unsafe) = {
  Ask.value()
}

let run(): i32 with(Supply, throws(bool)) = {
  unsafe {
    Ask.handle(value: { (resume) -> resume(42) }) {
      request()
    }
  }
}

let main(): i32 = {
  let result: Result(i32, bool) = try {
    Supply.handle(seed: { (resume) -> resume(0) }) { run() }
  }
  result ?? 0
}
