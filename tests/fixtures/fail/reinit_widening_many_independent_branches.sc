let Payload = struct(value: i32)
let Bundle = struct(
  f00: Payload,
  f01: Payload,
  f02: Payload,
  f03: Payload,
  f04: Payload,
  f05: Payload,
  f06: Payload,
  f07: Payload,
  f08: Payload,
  f09: Payload,
  f10: Payload,
  f11: Payload,
  f12: Payload,
  f13: Payload,
)

let consume(move value: Payload): () = { () }

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
  let mut bundle = Bundle(
    Payload(0),
    Payload(1),
    Payload(2),
    Payload(3),
    Payload(4),
    Payload(5),
    Payload(6),
    Payload(7),
    Payload(8),
    Payload(9),
    Payload(10),
    Payload(11),
    Payload(12),
    Payload(13),
  )
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
