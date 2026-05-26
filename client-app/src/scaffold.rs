use std::fs;
use std::io;
use std::path::Path;

/// Holds the core template variables required to scaffold a new application
pub struct AppServiceTemplate<'a> {
    pub app_name: &'a str,
    pub mac_address: &'a str,
    pub domain_name: &'a str,
}

/// Creates the necessary directory structure for a new application within a stack
pub fn create_app_dirs(stack_dir: &str, app_name: &str) -> io::Result<()> {
    let path = format!("stacks/{}/{}", stack_dir, app_name);
    let dir_path = Path::new(&path);

    if !dir_path.exists() {
        fs::create_dir_all(dir_path)?;
    }

    Ok(())
}

/// Generates a randomized MAC address for isolated container networking
pub fn generate_mac_address() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!(
        "02:42:ac:11:{:02x}:{:02x}",
        rng.gen_range(0..255),
        rng.gen_range(0..255)
    )
}

/// Returns the standard Watchtower docker-compose snippet
pub fn watchtower_service_yaml() -> &'static str {
    r#"
    watchtower:
        image: containrrr/watchtower
        volumes:
            - /var/run/docker.sock:/var/run/docker.sock
        command: --interval 86400
        restart: unless-stopped
"#
}

/// Returns the standard Promtail docker-compose snippet
pub fn promtail_service_yaml() -> &'static str {
    r#"
    promtail:
        image: grafana/promtail:latest
        volumes:
            - /var/log:/var/log
        restart: unless-stopped
"#
}

/// Returns the standard Traefik labels and network configuration snippet
pub fn traefik_service_yaml() -> &'static str {
    r#"
        labels:
            - "traefik.enable=true"
            - "traefik.http.routers.app.rule=Host(`${DOMAIN_NAME}`)"
            - "traefik.http.services.app.loadbalancer.server.port=80"
"#
}
// removed invalid YAML/config lines

/// Generate a full stack docker-compose.yml with selected default services
pub fn scaffold_stack_with_services(
    app: &AppServiceTemplate,
    include_watchtower: bool,
    include_promtail: bool,
    include_traefik: bool,
) -> String {
    let mut services = String::new();
    // Main app service
    services.push_str(&format!(
        "  {}:\n    image: nginx:latest\n    mac_address: {}\n",
        app.app_name, app.mac_address
    ));
    // Add selected defaults
    if include_watchtower {
        services.push_str(watchtower_service_yaml());
    }
    if include_promtail {
        services.push_str(promtail_service_yaml());
    }
    if include_traefik {
        services.push_str(traefik_service_yaml());
    }
    // Compose file header and networks
    services
}
