use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::models::Task;

pub fn save_tasks(path: &Path, tasks: &HashMap<u64, Task>) -> io::Result<()> {
    let json = serde_json::to_string_pretty(tasks)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

pub fn load_tasks(path: &Path) -> io::Result<HashMap<u64, Task>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(path)?;
    let tasks: HashMap<u64, Task> = serde_json::from_str(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(tasks)
}
