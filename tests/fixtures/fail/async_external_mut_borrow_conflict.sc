let main(): i32 = {
  let mut value = 41
  let reference: borrow(mut)(i32) = borrow(mut)(value)
  let future = async {
    let awaited = await child()
    reference + awaited
  }
  value = 0
  0
}

let child() = {
  async { 1 }
}
