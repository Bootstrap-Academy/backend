{
  settings.global.excludes = [
    "academy_assets/assets/*"
    "Cargo.nix"
    ".envrc"
    ".gitattributes"
    "justfile"
    "*.md"
    "*.pdf"
    "*.png"
    "*.sql"
    "*.toml"
  ];

  programs.black.enable = true;
  settings.formatter.black.options = [
    "--line-length=120"
    "--skip-magic-trailing-comma"
  ];

  programs.nixfmt.enable = true;
  programs.nixfmt.strict = true;

  programs.prettier.enable = true;

  programs.rustfmt.enable = true;
}
