let main(): i32 = {
  let future = async {
    loop {
      let value = await child()
      if value == 0 {
        continue()
      } else {
        ()
      }
    }
  }
  42
}

let child() = {
  async { 1 }
}
