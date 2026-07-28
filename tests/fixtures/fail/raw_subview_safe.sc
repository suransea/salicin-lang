let main(): i32 = {
  let text: string = "safe"
  let view = text.as_str()
  raw_subview(view, 0, 1)
  0
}
