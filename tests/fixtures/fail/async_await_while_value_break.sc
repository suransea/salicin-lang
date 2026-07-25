let main(): i32 = {
  let future = async {
    while { true } {
      let value = await child()
      if value == 0 {
        break(1)
      } else {
        continue()
      }
    }
  }
  0
}

let child() = {
  async { 1 }
}
