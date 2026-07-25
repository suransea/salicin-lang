let main(): i32 = {
  let future = async {
    let value = 41
    let reference: borrow(i32) = borrow(value)
    let awaited = await child()
    reference + awaited
  }
  0
}

let child() = { async { 1 } }
