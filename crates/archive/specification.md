## Specification

### Definitions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL
NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",  "MAY", and "OPTIONAL"
in this document are to be interpreted as described in
[IETF RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

The key word "BLAKE3" in this document is to be interpreted as
described in [BLAKE3-team/BLAKE3-specs](https://github.com/BLAKE3-team/BLAKE3-specs).

The key word "Ed25519" in this document is to be interpreted as
described in [IETF RFC 8032](https://datatracker.ietf.org/doc/html/rfc8032).

### Layout

```ebnf
archive = header, { object }, footer;

header = marker("hd"), u16(1);
footer = marker("ft"), postfix(
	digest({ object }),
	(x) = lenp({ digest(public-key), signature(private-key, x) })
);

object = marker("ob"), postfix((
	lenp(pathname(location)),
	u32(permissions),
	(
		// File
		u8(0), lenp(contents)
		// Symlink
	  | u8(1), lenp(target)
		// Directory
		| u8(2)
	)
), digest);

signature(private-key, x) = the Ed25519 signature of `marker("sg"), x` with `private-key`;
digest(b) = BLAKE3 hash of `x` with an output length of 32;
pathname(x) = Absolute pathname of `x` with no leading "/", and no "." or ".." segments;
marker(type) = "xuehua-archive@", type  where `type` is exactly 2 bytes
postfix(x, fn) = x, fn(x);
lenp(x) = postfix(x, (x) = u64(|x|));

u8(n)  = Little-Endian unsigned 8 bit integer;
u16(n) = Little-Endian unsigned 16 bit integer;
u32(n) = Little-Endian unsigned 32 bit integer;
u64(n) = Little-Endian unsigned 64 bit integer;
```

### Details

- **Ordering:** Parent directory objects MUST be emitted before their children objects.
- **Paths:** `object`s MUST be sorted by the bytes of their
	`location` in ascending order. Duplicate `location`'s' MUST NOT appear.
