use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static FILE: OnceLock<Mutex<File>> = OnceLock::new();

/// Opens (or creates) the log file at `path`.
/// Pass `append = true` to add to an existing file, `false` to truncate it.
pub fn init(path: &str, append: bool) -> std::io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(append)
        .truncate(!append)
        .write(!append)
        .open(path)?;
    FILE.set(Mutex::new(file)).ok();
    Ok(())
}

/// Returns a compact UTC timestamp: `HH:MM:SS.mmm`
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
}

pub fn write_line(line: &str) {
    let formatted = format!("[{}] {}", timestamp(), line);
    if let Some(f) = FILE.get() {
        writeln!(f.lock().unwrap(), "{}", formatted).ok();
    } else {
        println!("{}", formatted);
    }
}
