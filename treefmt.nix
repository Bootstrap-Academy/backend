{ lib, pkgs, ... }:

{
  tree-root-file = ".git/config";
  on-unmatched = "fatal";

  excludes = [
    "academy_assets/assets/*"
    "Cargo.nix"
    ".envrc"
    ".gitattributes"
    "*/.gitattributes"
    ".gitignore"
    "justfile"
    "*.lock"
    "*.md"
    "*.sql"
  ];

  formatter.black = {
    command = lib.getExe pkgs.black;
    includes = [ "*.py" ];
    options = [
      "--line-length=120"
      "--skip-magic-trailing-comma"
    ];
  };

  formatter.nixfmt = {
    command = lib.getExe pkgs.nixfmt;
    includes = [ "*.nix" ];
    options = [ "--strict" ];
  };

  formatter.prettier = {
    command = lib.getExe pkgs.prettier;
    includes = [
      "*.json"
      "*.yml"
    ];
    options = [ "--write" ];
  };

  formatter.rustfmt = {
    command = lib.getExe pkgs.rustfmt;
    includes = [ "*.rs" ];
    options = [
      "--config=skip_children=true"
      "--edition=2024"
    ];
  };

  formatter.taplo = {
    command = lib.getExe pkgs.taplo;
    includes = [ "*.toml" ];
    options = [
      "format"
      "--option=column_width=120"
      "--option=align_comments=false"
    ];
  };
}
