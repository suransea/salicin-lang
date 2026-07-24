let main(): i32 = {
  let value = Some(42)
  match value
    { Some(found) -> found }
    { _ -> 0 }
  found
}
