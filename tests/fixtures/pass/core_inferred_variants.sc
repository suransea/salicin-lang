let option = core.option
let result = core.result

let main(): i32 = {
  let some = option.some(20)
  let none: option(i32) = option.none
  let ok = result(e: bool).ok(22)
  let err: result(bool)(i32) = result.err(false)

  let from_some = match some
    { some(value) -> value }
    { none -> 0 }
  let from_none = match none
    { some(value) -> value }
    { none -> 0 }
  let from_ok = match ok
    { ok(value) -> value }
    { err(_) -> 0 }
  let from_err = match err
    { ok(value) -> value }
    { err(_) -> 0 }
  from_some + from_none + from_ok + from_err
}

test("core_inferred_variants.sc") {
  main() == 42
}
