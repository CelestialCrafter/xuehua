# xuehua

## Abstract

A package manager and (eventually) distro inspired by nixpkgs/nixos.

## Concepts

**namespace**
a repository or group name for packages (ex. xuehua, alpine, debian, celestial)
`local` is a special name that means "the current namespace"

**package**
a namespace, name, and version in the format of `[path to folder containing xuehua.toml/]<namespace>:<name>[@version]`

**store**
a read-only (writable by `xh`) cache containing packages that have been linked/are linked

## commands

`xh shell <package>`:
sets up a sandboxed environment for a package,
and drops the user into it

`xh link <package>`:
sets up symlinks for a package in the running system

`xh unlink <package>`:
removes symlinks for a pcakage in the running system

`xh gc`:
removes unlinked packages from the store

## package api

**dependencies**

- `addBuild(pkg)` → build-time deps
- `addRuntime(pkg)` → runtime deps (could alias `addBuild`)
- `forceResolution(pkg, repo)` → local conflict resolution

**fetch**

- `network(url) -> store path`: fetch remote
- `local(path) -> store path`: fetch local

**unpack**

- `auto(path) -> path`: detect type & unpack
- `git(url) -> path`: clone
- `tar/xz/gzip/zstd(path) -> path`: unpack respective format

**commands**

- `shell(package, file, args) -> stdio handle`: run commands

bubblewrap sandbox by default (can be disabled from xuehua.toml)

## build cache

a regular store hosted on the internet, with an attestation layer for verification.

### package submit flow

- builder produces package,
- builder requests a new package
- attestation server verifies that the builder is in a in configured list of trusted builders
- attestation server sets up submissions for the package
- builder submits an attestation

## client flow

- randomly probe attestation servers until one confirms the package is registered
- client fetches positive/negative attestation counts, and logs them
- client fetches essential attestations, and verifies that:
  - no essential attester has yanked the package
  - at least n essential attestations exist
  - all essential attestations are properly signed
  - the builder has attested its own package

  if either one of these assertions fail:
    1. the user is strongly warned
    1. the cache artifact is refused
    1. the build fails
  this may be overriden with a flag (eg. --allow-insecure)
- if all assertions pass, the build cache's package is accepted

## client flow (attest mode)

- package is built locally
- the client submits an attestation to every configured attestation server

## threat model

### malicious build server

**threat**:
attackers can publish malicious packages, and infect users

**mitigation**:
- require n essential attestations before clients are able to use the cache for a package
- if a build server forges an attestion (and somehow gets thru the trusted attesters),
  an abnormal amount of negative non-essential attestations can serve as an alarm

### malicious attestation server

a malicious attestation server may censor attestations, or lie about counts.

**threat**:
this allows compromised build servers to be trusted by clients

**mitigation**:
- have clients submit attestations to multiple servers to prevent censorship
- let clients audit attestion servers by verifying the signatures within all attestions

### attestation spam

an attacker could spam the attestation server with positive or negative attestations

**threat**:
this wastes trusted attesters time,
and potentially gives users false trust in a package

**mitigation**:
require clients to solve a proof of work challenge on every attestation,
optionally slightly lowering the difficulty for users who have attested a variety of packages

### compromised trusted attester

**threat**:
this allows attackers to revoke valid packages,
and give trusted attestations of malicious packages

**mitigation**:
- rotate keys regularly, with an expiry on every key
- clients can drop trust in a compromised trusted attester
- and n trusted attesters can gather to revoke a compromised trusted attesters authority

### malicious package

**threat**:
a package that might've not seemed malicious at first,
but was trusted, then turned out to be malicious, can infect users

**mitigation**:
trusted attesters must have the power to yank packages

## further optimizations

- using a distributed log on attestation servers to prevent tampering (con: makes server setup harder)
- make clients request multiple attestation servers to cross-verify their results (useless if distributed log is implemented)

## specifics

- once a package is yanked, there is no way to revoke the yank
- a positive attestation agrees with builder's statement, and a negative disputes it
- the essential attesters group is comprised of the builder itsself, and the trusted attesters
- an attestation is a signed statement: "at timestamp T, when i built package P with inputs I, the output was O"
- clients can configure their own:
  - attestation servers
  - build caches
  - trusted attesters
  - \# of required essential attestations
