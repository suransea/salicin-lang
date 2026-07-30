let option_is(value: core.option(u64), expected: u64): bool = {
  match value
    { some(value) -> value == expected }
    { none -> false }
}

let borrowed_checks(): bool = {
  let text: string = "A柳B🙂"
  let prefix: string = "A柳"
  let suffix: string = "B🙂"
  let needle: string = "柳B"
  let missing: string = "C"
  let empty: string = ""
  let view = text.as_str()
  let prefix_view = prefix.as_str()
  let suffix_view = suffix.as_str()
  let needle_view = needle.as_str()
  let missing_view = missing.as_str()
  let empty_view = empty.as_str()
  view.starts_with(prefix_view) &&
    view.ends_with(suffix_view) &&
    view.contains(needle_view) &&
    !view.contains(missing_view) &&
    option_is(view.find(needle_view), 1) &&
    option_is(view.find(empty_view), 0)
}

let owning_checks(): bool = {
  let text: string = "A柳B🙂"
  let prefix: string = "A"
  let suffix: string = "🙂"
  let needle: string = "B🙂"
  let prefix_view = prefix.as_str()
  let suffix_view = suffix.as_str()
  let needle_view = needle.as_str()
  let selected = match text.substring(1, 4)
    { some(value) ->
      let expected: string = "柳"
      value == expected && value.capacity() == 3
    }
    { none -> false }
  let invalid = match text.substring(2, 4)
    { some(_) -> false }
    { none -> true }
  selected &&
    invalid &&
    text.starts_with(prefix_view) &&
    text.ends_with(suffix_view) &&
    option_is(text.find(needle_view), 4)
}

let ordering_checks(): bool = {
  let ascii: string = "A"
  let latin: string = "é"
  let cjk: string = "柳"
  let emoji: string = "🙂"
  let ascii_view = ascii.as_str()
  let latin_view = latin.as_str()
  let cjk_view = cjk.as_str()
  let emoji_view = emoji.as_str()
  ascii < latin &&
    latin < cjk &&
    cjk < emoji &&
    ascii_view < latin_view &&
    latin_view < cjk_view &&
    cjk_view < emoji_view
}

let main(): i32 = {
  if borrowed_checks() &&
    owning_checks() &&
    ordering_checks() {
    42
  } else {
    0
  }
}

test("string_search.sc") {
  std.test.assert(main() == 42)
}
