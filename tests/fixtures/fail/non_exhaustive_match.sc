let Choice = enum {
  Yes,
  No,
}

let choose(value: Choice): i32 = { value match {
  Choice.Yes => 42,
}
}

let main(): i32 = { choose(Choice.Yes) }
