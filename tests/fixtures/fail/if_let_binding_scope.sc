let main(): i32 = {
  let value = some(42)
  match value
    { some(found) -> found }
    { _ -> 0 }
  found
}
