let main(): i32 = {
  let future = async {
    let value = 41
    let first: borrow(i32) = borrow(value)
    let second: borrow(i32) = first
    let awaited = await child()
    second + awaited
  }
  0
}

let child() = {
  async { 1 }
}
