use std::env;
use std::path::PathBuf;
use std::process::{Command, exit};

fn project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().to_path_buf()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match cmd {
        "tui" => {
            let mut c = Command::new("cargo");
            c.args(["run", "-p", "angelica-tui"]);
            for a in &args[1..] {
                c.arg(a);
            }
            run(&mut c);
        }
        "gui" => {
            let tauri_dir = project_root().join("angelica-gui").join("src-tauri");
            if !tauri_dir.exists() {
                eprintln!("error: {} not found", tauri_dir.display());
                exit(1);
            }
            let mut c = Command::new("cargo");
            c.args(["tauri", "dev"]);
            c.current_dir(&tauri_dir);
            run(&mut c);
        }
        "-h" | "--help" | "help" => {
            println!("angelica xtask — unified dev entry point\n");
            println!("USAGE:");
            println!("  cargo run -p xtask -- <subcommand> [args]\n");
            println!("SUBCOMMANDS:");
            println!("  tui          Start TUI (pass -- --debug for debug server)");
            println!("  gui          Start GUI (Tauri desktop window)");
            println!("  check        cargo check --workspace");
            println!("  fmt          cargo fmt && cargo clippy");
            println!("  test         cargo test\n");
            println!("EXAMPLES:");
            println!("  cargo run -p xtask -- tui -- --debug --log-level debug");
            println!("  cargo run -p xtask -- gui");
        }
        "check" => {
            run(Command::new("cargo").args(["check", "--workspace"]));
        }
        "fmt" => {
            run(Command::new("cargo").arg("fmt"));
            run(Command::new("cargo").args(["clippy", "--workspace"]));
        }
        "test" => {
            run(Command::new("cargo").arg("test"));
        }
        _ => {
            eprintln!("unknown subcommand: {cmd}");
            eprintln!("use 'cargo run -p xtask -- help' for usage");
            exit(1);
        }
    }
}

fn run(c: &mut Command) {
    let status = c.status().expect("failed to execute command");
    if !status.success() {
        exit(status.code().unwrap_or(1));
    }
}
