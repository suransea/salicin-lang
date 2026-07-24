let Choice = enum {
  Yes,
  No,
}

let choose(value: Choice): i32 = { match value
  { Choice.Yes -> 42 }
}

let main(): i32 = { choose(Choice.Yes) }
