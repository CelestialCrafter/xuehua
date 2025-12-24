## Specification

### Definitions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL
NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",  "MAY", and "OPTIONAL"
in this document are to be interpreted as described in
[IETF RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

The key word "BLAKE3" in this document is to be interpreted as
described in [BLAKE3-team/BLAKE3-specs](https://github.com/BLAKE3-team/BLAKE3-specs).

### Layout

```ebnf
xhar = magic, index, { digest(object-contents) };

magic = "xuehua-archive", u16(1);

index = digest({ lenp(index-entry) });

// location = pathname
object-metadata = lenp(location), u32(permissions), u64(|object-contents|) object-type;

object-type
	// File
	= u8(0)
	// Symlink
	| u8(1)
	// Directory
	| u8(2)
	;

object-contents
	// File
	= contents
	// Symlink
  | target
	// Directory
	| ()
	;

() = unit function;
lenp(x) = u64(|x|), x;
digest(b) = x, BLAKE3 hash of x with an output length of 32;

u8(n)  = little-endian unsigned 8 bit integer;
u16(n) = little-endian unsigned 16 bit integer;
u32(n) = little-endian unsigned 32 bit integer;
u64(n) = little-endian unsigned 64 bit integer;
```

### Details

- **Ordering:** Parent directory objects MUST be emitted before their children objects.
- **Paths:** `index` entries MUST be sorted by the bytes of their
	`location` in ascending order.
	Duplicate `location`'s' MUST NOT appear.
	`location`s MUST NOT have a leading "/".
	`location`s MUST NOT contain "." or ".." segments.
- **Hashing:** When computing the digest of a complete archive, implementations MUST hash an aggregate of all existing digests.
