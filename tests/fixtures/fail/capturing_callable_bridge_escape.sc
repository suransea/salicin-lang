let escape(move action: (): i32): (): i32 = { action }

let main(): i32 = {
  let value = 42
  let escaped = escape({ value })
  escaped()
}
