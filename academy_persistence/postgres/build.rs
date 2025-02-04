use std::{
    collections::BTreeMap,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo::rerun-if-changed=migrations");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let migrations_rs = out_dir.join("migrations.rs");
    emit_migrations(&migrations_rs);
    println!("cargo::rustc-env=MIGRATIONS={}", migrations_rs.display());
}

fn emit_migrations(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = BufWriter::new(file);
    write!(&mut writer, "&[").unwrap();
    for (name, (up, down)) in collect_migrations() {
        write!(
            &mut writer,
            "Migration{{name:{name:?},up:{up:?},down:{down:?}}},"
        )
        .unwrap();
    }
    write!(&mut writer, "]").unwrap();
    writer.flush().unwrap();
}

fn collect_migrations() -> BTreeMap<String, (String, String)> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .read_dir()
        .unwrap()
        .map(|file| {
            let file = file.unwrap();
            let name = file.file_name().into_string().unwrap();

            let up = std::fs::read_to_string(file.path().join("up.sql")).unwrap();
            let down = std::fs::read_to_string(file.path().join("down.sql")).unwrap();

            (name, (up, down))
        })
        .collect()
}
