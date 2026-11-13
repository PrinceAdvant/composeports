use std::process::ExitCode;

fn main() -> ExitCode {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() || files.iter().any(|f| f == "-h" || f == "--help") {
        eprintln!("usage: composeports compose.yml [other/compose.yml ...]");
        eprintln!("Lists published host ports and exits 1 if any two bindings conflict.");
        return ExitCode::from(2);
    }
    let mut all = Vec::new();
    for f in &files {
        let text = match std::fs::read_to_string(f) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{f}: {e}");
                return ExitCode::from(2);
            }
        };
        match composeports::parse(&text, f) {
            Ok(b) => all.extend(b),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        }
    }
    all.sort_by_key(|b| (b.host_port, b.protocol.clone()));
    for b in &all {
        let ip = if b.host_ip.is_empty() { "*" } else { &b.host_ip };
        println!("{:>5}/{:<3} {:<15} -> {:<5} {} ({})", b.host_port, b.protocol, ip, b.container_port, b.service, b.file);
    }
    let c = composeports::conflicts(&all);
    for (a, b) in &c {
        println!("CONFLICT {}/{}: {} ({}) and {} ({})", a.host_port, a.protocol, a.service, a.file, b.service, b.file);
    }
    if c.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}
