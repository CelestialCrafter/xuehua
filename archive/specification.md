# Xuehua Archive

## Definitions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL
NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",  "MAY", and "OPTIONAL"
in this document are to be interpreted as described in
[IETF RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

The key word "Zstandard" in this document is to be interpreted as described in
[IETF RFC 8878](https://datatracker.ietf.org/doc/html/rfc8878).

The key word "pathname" in this document is to be interpreted as
an absolute pathname as described in Section 4.11 of
[IEEE Std 1003.1-2024](https://ieeexplore.ieee.org/document/974398).

The key word "BLAKE3" in this document is to be interpreted as
described in [BLAKE3-team/BLAKE3-specs](https://github.com/BLAKE3-team/BLAKE3-specs).

## Overview

```ebnf
xhar = magic, index, { object };

magic = "xuehua-archive", u16(1);

// `location` is a pathname
index = lenp({ lenp(location) }), digest({ lenp(location) });

object
	// Create
	// [statx](https://man7.org/linux/man-pages/man2/statx.2.html) fields
	// permissions = stx_mode's permission bits
	// created     = stx_btime
	// modified    = stx_mtime
	// TODO: permissions needs to be included in the digest
	= u8(0), object-metadata, object-body
	// Delete
	| u8(1)
	;
object-body = (
	// File
	u8(0), zstd-dict(contents), lenp(zstd-stream(contents)), digest(object-metadata, contents)
	// Symlink
	// `target` is a pathname
  | u8(1), lenp(target), digest(object-metadata, target)
  // Directory
  | u8(2), digest(object-metadata)
);
object-metadata = u32(permissions.rwx), timestamp(modified);

lenp(x) = u64(|x|), x;
digest(b) = BLAKE3 hash of b with an output length of 32;
timestamp(t) = i64(t.seconds), u32(t.nanoseconds);

zstd-stream(b) = Zstandard stream of b;
zstd-dict(b)
	// No dictionary
	= u8(0)
	// Dictionary
	| u8(1), lenp(zstd-dict-inner(b))
	// External dictionary
	| u8(2), digest(zstd-dict-inner(b))
	;
zstd-dict-inner(b) = Zstandard dictionary used to compress `b`;

u8(n)  = little-endian unsigned 8 bit integer;
u16(n) = little-endian unsigned 16 bit integer;
u32(n) = little-endian unsigned 32 bit integer;
u64(n) = little-endian unsigned 64 bit integer;
i64(n) = little-endian signed 64 bit integer;
```

# Details

- **Directory Order:** Parent directory objects MUST be emitted before their children objects.
- **Sorting:** `object-map` entries MUST be sorted by the bytes of their
	`location` in ascending order. Duplicate `location` entries MUST NOT appear.
- **Deletion**: If applicable, deletion objects MUST recursively remove children.
- **Dictionary Training:** When training a dictionary, it MAY be trained on a file object's `contents`.
	See [Zstandard as a patching engine](https://github.com/facebook/zstd/wiki/Zstandard-as-a-patching-engine) for more information.
- **External Dictionaries:** For external dictionaries, decoders MUST inject the
	dictionary identified by the digest. Implementations SHOULD provide a method to
	dynamically locate dictionaries.
- **Complete Digest:** When computing the semantic digest of an archive, implementations SHOULD hash an ordered composite of:
	- File content digests
	- Symlink target digests
	- Location digest
