use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static FILE: OnceLock<Mutex<File>> = OnceLock::new();

pub fn init(path: &str) -> std::io::Result<()> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    FILE.set(Mutex::new(file)).ok();
    Ok(())
}

pub fn write_line(line: &str) {
    if let Some(f) = FILE.get() {
        writeln!(f.lock().unwrap(), "{}", line).ok();
    } else {
        println!("{}", line);
    }
}
