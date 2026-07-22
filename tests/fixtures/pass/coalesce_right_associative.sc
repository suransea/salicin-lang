use core.Option

let main(): i32 = {
  let first = Option(i32).None
  let second = Option(i32).Some(42)
  first ?? second ?? 0
}
