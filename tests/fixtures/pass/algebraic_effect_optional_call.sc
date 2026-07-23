use std.Option
use std.Result

let Read = effect {
  let option_base(present: bool): Option(Adder)
  let result_base(present: bool): Result(bool)(Adder)
  let argument(): i32
}

let Adder = struct { base: i32 }

extend Adder {
  let add(self)(value: i32): i32 = { self.base + value }
}

let main(): i32 = {
  let mut arguments = 0
  let result: i32 = Read.handle(
    option_base: { (present, resume) ->
      resume(if present { Option.Some(Adder { base: 8 }) } else { Option.None })
    },
    result_base: { (present, resume) ->
      resume(if present { Result.Ok(Adder { base: 8 }) } else { Result.Err(true) })
    },
    argument: { (resume) ->
      arguments += 1;
      resume(2)
    },
  ) {
    let option_some = Read.option_base(true)?.add(Read.argument()) ?? 0
    let option_none = Read.option_base(false)?.add(Read.argument()) ?? 10
    let result_ok = Read.result_base(true)?.add(Read.argument()) ?? 0
    let result_err = Read.result_base(false)?.add(Read.argument()) ?? 10
    option_some + option_none + result_ok + result_err
  }
  result + arguments
}
