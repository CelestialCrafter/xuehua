{ pkgs, lib, ... }:

with lib;
let
  toSnakeCase =
    str:
    let
      # split on boundaries where lowercase is followed by uppercase
      # and then map everything to lowercase
      parts = map toLower (
        lib.splitStringBy (prev: curr: match "[a-z]" prev != null && match "[A-Z]" curr != null) true str
      );
    in
    concatStringsSep "_" (map (addContextFrom str) parts);
  transformBool = value: (if typeOf value == "bool" then if value then "yes" else "no" else value);
in
{
  confd =
    name: attrs:
    pkgs.writeText ("openrc-confd-" + name) (
      generators.toKeyValue { } (
        mapAttrs' (
          name: value: nameValuePair (toSnakeCase name) "\"${escapeShellArg (transformBool value)}\""
        ) attrs
      )
    );
}
