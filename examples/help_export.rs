//! Writes the Russian topics of the help files in HELP_DIR to OUT_DIR, one `<FILE>.txt` each in
//! the translation format (`gmdss::help::HelpFile::russian`). Put the translated files in a
//! `help_en` folder beside `help`, and the Books page shows them in English.
//!
//! `cargo run --example help_export -- HELP_DIR OUT_DIR`

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out] = &args[..] else {
        eprintln!("usage: help_export HELP_DIR OUT_DIR");
        std::process::exit(2);
    };
    std::fs::create_dir_all(out)?;
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        let Some(h) = std::fs::read(&p).ok().and_then(|b| gmdss::help::parse(&b)) else { continue };
        let ru = h.russian();
        if !ru.is_empty() {
            let name = p.file_stem().unwrap_or_default().to_string_lossy().to_ascii_uppercase();
            println!("{name}: {} topics, {} chars", ru.matches("@@ ").count(), ru.chars().count());
            std::fs::write(std::path::Path::new(out).join(format!("{name}.txt")), ru)?;
        }
    }
    Ok(())
}
