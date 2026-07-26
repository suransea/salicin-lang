let option = std.option
let result = std.result

let read = effect {
  let option_base(present: bool): option(adder)
  let result_base(present: bool): result(bool)(adder)
  let argument(): i32
}

let adder = struct { base: i32 }

extend adder {
  let add(self)(value: i32): i32 = { self.base + value }
}

let main(): i32 = {
  let mut arguments = 0
  let result: i32 = read.handle option_base { (present, resume) ->
      resume(if present { option.some(adder { base: 8 }) } else { option.none })
    } result_base { (present, resume) ->
      resume(if present { result.ok(adder { base: 8 }) } else { result.err(true) })
    } argument { (resume) ->
      arguments += 1;
      resume(2)
    } action {
      let option_some = read.option_base(true)?.add(read.argument()) ?? 0
      let option_none = read.option_base(false)?.add(read.argument()) ?? 10
      let result_ok = read.result_base(true)?.add(read.argument()) ?? 0
      let result_err = read.result_base(false)?.add(read.argument()) ?? 10
      option_some + option_none + result_ok + result_err
    }
  result + arguments
}

test("algebraic_effect_optional_call.sc") {
  main() == 42
}
