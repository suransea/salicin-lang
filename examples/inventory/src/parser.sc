let string = core.string.string

/// Preserves the canonical runtime string while crossing a module boundary.
pub(package) let decode_name(move name: string): string = {
  name
}
