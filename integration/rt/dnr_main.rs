fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--version" || s == "-V") {
        println!(
            "dnr 0.1.0 (Deno 2.9.7; Laufey 0.7.0; format 1; backend {})",
            env!("DNR_BACKEND")
        );
        return;
    }
    if args.is_empty() || args.first().is_some_and(|s| s == "--help" || s == "-h") {
        println!(
            "dnr <script.ts|application.dnp> [args...]\n\nShared Deno runtime. Local modules and prepared node_modules only.\nDesktop activates on GUI API use. Applications run with full permissions."
        );
        return;
    }
    denort::dnr_desktop::main(args);
}
