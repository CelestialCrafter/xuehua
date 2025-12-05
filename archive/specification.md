# Xuehua Archive

## Definitions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL
NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",  "MAY", and "OPTIONAL"
in this document are to be interpreted as described in
[IETF RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

The key word "Zstandard" in this document is to be interpreted as described in
[IETF RFC 8878](https://datatracker.ietf.org/doc/html/rfc8878).

The key word "pathname" and "stat" in this document is to be interpreted as
an absolute pathname as described in Section 4.16 and sys/stat.h of
[IEEE Std 1003.1-2024](https://pubs.opengroup.org/onlinepubs/9799919799/) respectively.

The key word "BLAKE3" in this document is to be interpreted as
described in [BLAKE3-team/BLAKE3-specs](https://github.com/BLAKE3-team/BLAKE3-specs).

## Overview

```ebnf
xhar = magic, index, { operation, digest(operation) };

magic = "xuehua-archive", u16(1);

// `location` is a pathname
index = lenp({ lenp(location) }), digest({ lenp(location) });

operation
	// Create
	// permissions = stat's st_mode permission bits
	= u8(0), u32(permissions.rwx), (
		// File
		// When computing the digest of a file, the following should be hashed:
		// u8(0), lenp(contents)
		u8(0), zstd-dict(contents), lenp(zstd-stream(contents))
		// Symlink
		// `target` is a pathname
	  | u8(1), lenp(target)
	  // Directory
	  | u8(2)
	)
	// Delete
	| u8(1)
	;

lenp(x) = u64(|x|), x;
digest(b) = BLAKE3 hash of b with an output length of 32;

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
