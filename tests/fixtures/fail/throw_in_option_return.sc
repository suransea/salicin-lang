let option = std.option

let fail(): option(i32) = {
  throw(true)
}

let main(): i32 = { 42 }
