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
  programs.rustfmt.edition = "2024";

  programs.taplo.enable = true;
  settings.formatter.taplo.options = [
    "--option=column_width=120"
    "--option=align_comments=false"
  ];
}
