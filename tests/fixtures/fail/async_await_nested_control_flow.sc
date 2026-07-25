let main(): i32 = {
  let future = async {
    if true { await child() } else { 0 }
  }
  0
}

let child() = { async { 1 } }
