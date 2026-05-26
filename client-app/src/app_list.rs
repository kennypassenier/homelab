// Utility to list apps (subfolders with docker-compose.yml) for a given stack
use std::fs;
use std::path::Path;

pub fn list_apps_for_stack(stack_name: &str) -> Vec<String> {
    let mut apps = Vec::new();
    let stack_path = format!("stacks/{}", stack_name);
    if let Ok(entries) = fs::read_dir(&stack_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let compose = path.join("docker-compose.yml");
                if compose.exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        apps.push(name.to_string());
                    }
                }
            }
        }
    }
    apps.sort();
    apps
}
