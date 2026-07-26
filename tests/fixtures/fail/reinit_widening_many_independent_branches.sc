let payload = struct { value: i32 }
let bundle = struct { f00: payload,
  f01: payload,
  f02: payload,
  f03: payload,
  f04: payload,
  f05: payload,
  f06: payload,
  f07: payload,
  f08: payload,
  f09: payload,
  f10: payload,
  f11: payload,
  f12: payload,
  f13: payload, }

let consume(move value: payload): () = { () }

let stress(
  b00: bool,
  b01: bool,
  b02: bool,
  b03: bool,
  b04: bool,
  b05: bool,
  b06: bool,
  b07: bool,
  b08: bool,
  b09: bool,
  b10: bool,
  b11: bool,
  b12: bool,
  b13: bool,
): i32 = {
  let mut bundle = bundle { left: payload { value: 0 }, right: payload { value: 1 }, field2: payload { value: 2 }, field3: payload { value: 3 }, field4: payload { value: 4 }, field5: payload { value: 5 }, field6: payload { value: 6 }, field7: payload { value: 7 }, field8: payload { value: 8 }, field9: payload { value: 9 }, field10: payload { value: 10 }, field11: payload { value: 11 }, field12: payload { value: 12 }, field13: payload { value: 13 } }
  if b00 { consume(bundle.f00) }
  if b01 { consume(bundle.f01) }
  if b02 { consume(bundle.f02) }
  if b03 { consume(bundle.f03) }
  if b04 { consume(bundle.f04) }
  if b05 { consume(bundle.f05) }
  if b06 { consume(bundle.f06) }
  if b07 { consume(bundle.f07) }
  if b08 { consume(bundle.f08) }
  if b09 { consume(bundle.f09) }
  if b10 { consume(bundle.f10) }
  if b11 { consume(bundle.f11) }
  if b12 { consume(bundle.f12) }
  if b13 { consume(bundle.f13) }
  bundle.f13.value
}

let main(): i32 = { stress(
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
    false,
  )
}
