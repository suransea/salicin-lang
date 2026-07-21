let Some(x: i32): i32 = x

let from_function(): Option(i32) = Some(20)

let from_local(): Option(i32) = {
  let Some = 22
  Some
}

let from_branch(flag: bool): Option(i32) = {
  let Some = 11
  if flag { Some } else { Some }
}

let main(): i32 = {
  let direct = (from_function() ?? 0) + (from_local() ?? 0)
  let branch = (from_branch(true) ?? 0) + (from_branch(false) ?? 0)
  if direct == 42 && branch == 22 { 42 } else { 0 }
}
