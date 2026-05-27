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
            let gui_dir = project_root().join("angelica-gui");
            if !gui_dir.join("src-tauri").exists() {
                eprintln!("error: {} not found", gui_dir.join("src-tauri").display());
                exit(1);
            }
            let mut c = Command::new("cargo");
            c.args(["tauri", "dev"]);
            if !&args[1..].is_empty() {
                c.arg("--");
            }
            for a in &args[1..] {
                c.arg(a);
            }
            c.current_dir(&gui_dir);
            run_gui(&mut c);
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
            println!("  cargo run -p xtask -- gui -- --debug");
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

fn run_gui(c: &mut Command) {
    let status = c
        .stdin(std::process::Stdio::null())
        .status()
        .expect("failed to execute command");
    let code = status.code().unwrap_or(1);
    let _ = Command::new("stty")
        .arg("sane")
        .stdin(std::process::Stdio::inherit())
        .status();
    if !status.success() {
        exit(code);
    }
}
