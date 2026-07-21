let run(base: i32): i32 = {
  let add_base = { (increment: i32) -> base + increment }
  add_base(2)
}

let main(): i32 = { run(40) }
