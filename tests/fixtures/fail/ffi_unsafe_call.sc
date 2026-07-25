extern "C" {
  @link_name("abs")
  let c_abs(value: i32): i32
}

let main(): i32 = { c_abs(-42) }
