fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--version" || s == "-V") {
        println!(
            "dnr 0.3.0 (Deno 2.9.7; Laufey 0.7.0; formats 1,2,3; backend {})",
            env!("DNR_BACKEND")
        );
        return;
    }
    if args.is_empty() || args.first().is_some_and(|s| s == "--help" || s == "-h") {
        println!(
            "dnr [--no-code-cache] [--no-transpile-cache] <script.ts|application.dnp|installed-directory> [args...]\ndnr tree <application.dnp>\ndnr extract <application.dnp> <directory>\ndnr install <application.dnp> [directory] [--mode native|full] [--force]\ndnr cache <list|info|clean|rebuild> [options]\n\nShared Deno runtime. Local modules and prepared node_modules only.\nDesktop activates on GUI API use. Applications run with full permissions.\nTree includes ZIP metadata; extract requires a new or empty directory.\nNative groups: dnc scan and --package-config; see docs/NATIVE-PACKAGING.md.\nUse explicit paths (./tree, ./install, ./cache) for scripts with command names."
        );
        return;
    }
    if args.first().is_some_and(|s| s == "tree" || s == "extract") {
        if let Err(error) = package_command(&args) {
            eprintln!("dnr: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    match dnr_package::commands::run(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("dnr: {error:#}");
            std::process::exit(1);
        }
    }
    denort::dnr_desktop::main(args);
}

fn package_command(args: &[String]) -> deno_core::anyhow::Result<()> {
    use deno_core::anyhow::ensure;
    use std::{io::Write, path::Path};
    let tree = args[0] == "tree";
    let usage = if tree {
        "usage: dnr tree <application.dnp>"
    } else {
        "usage: dnr extract <application.dnp> <directory> (new or empty)"
    };
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        println!("{usage}");
        return Ok(());
    }
    ensure!(args.len() == if tree { 2 } else { 3 }, "{usage}");
    let package = dnr_package::Package::open(Path::new(&args[1]), 0)?;
    if tree {
        let mut output = std::io::BufWriter::new(std::io::stdout().lock());
        package.write_tree(&mut output)?;
        output.flush()?;
    } else {
        package.extract_to(Path::new(&args[2]))?;
        println!("Extracted {} to {}", args[1], args[2]);
    }
    Ok(())
}
