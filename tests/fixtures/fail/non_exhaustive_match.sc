let choice = enum {
  yes,
  no,
}

let choose(value: choice): i32 = { match value
    { choice.yes -> 42 }
}

let main(): i32 = { choose(choice.yes) }
