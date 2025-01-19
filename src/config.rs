use std::collections::HashMap;
use std::error::Error;
use std::fs;

use crate::server::Server;

pub fn parse_config(file_path: &str) -> Result<HashMap<String, Server>, Box<dyn Error>> {
    let config_str = fs::read_to_string(file_path)?;

    let config: HashMap<String, Server> = serde_yaml::from_str(&config_str)?;

    Ok(config)
}
