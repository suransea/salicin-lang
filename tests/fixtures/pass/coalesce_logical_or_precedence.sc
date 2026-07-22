use core.Option

let main(): i32 = {
  let present = Option(bool).Some(false)
  if present ?? false || true { 0 } else { 42 }
}
