# Comparison with Nix

Given that Xuehua was born out of frustration for [NixOS](https://nixos.org/),
it's worth comparing their differences.

## Language

NixOS is tightly coupled to the Nix expression language, which in my opinion
has poor tooling, errors and performance with a high learning curve. Everything
Nix provides is deeply integrated with the language, and cannot be changed or swapped out.

Xuehua's Engine is language agnosic, allowing the planning frontend
to be decoupled from the core. While the Package Manager uses
[Lua](https://www.lua.org/) for its performance, simplicity, and embedability,
the Engine can be driven by any language or data format.

## Evaluation & Transparency

NixOS mangles planning and building together. If you want to evaluate a
derivation to inspect its contents, Nix may need to implicitly download some
sources, copy stuff to your systems store, and possibly even compile some things
*just* to inspect some packages. This is often a  slow and opaque process, and
it obscures all of the downloads and compilations needed until *after* you've
already done it all.

Xuehua defines a boundary between planning package definitions and actually
building the packages. The engine requires a fully resolved package graph before
any build steps are executed. This is more restrictive, but allows for a near
instant view into exactly what you're about to build and why.

TODO: compare philosophy
TODO: compare compatability approaches
TODO: compare complexity
