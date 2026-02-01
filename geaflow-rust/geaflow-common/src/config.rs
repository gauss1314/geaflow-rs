use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Configuration {
    config: HashMap<String, String>,
}

impl Configuration {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
        }
    }

    pub fn put(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.config.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    pub fn get_string(&self, key: &str, default_value: &str) -> String {
        self.config
            .get(key)
            .cloned()
            .unwrap_or_else(|| default_value.to_string())
    }
}
