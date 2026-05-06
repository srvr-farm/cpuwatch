fn main() {
    if let Err(error) = cpuwatch::run() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}
