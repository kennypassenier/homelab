use std::fs;
use std::path::Path;
/// Creates the GitOps and config directories for a new app.
///
/// - `stack_dir`: e.g. "media"
/// - `app_name`: e.g. "jellyfin"
///
/// This will create:
///   stacks/<stack_dir>/<app_name>/
///   stacks/<stack_dir>/<app_name>-config/
pub fn create_app_dirs(stack_dir: &str, app_name: &str) -> std::io::Result<()> {
  let base = Path::new("stacks").join(stack_dir);
  let app_dir = base.join(app_name);
  let config_dir = base.join(format!("{}-config", app_name));
  // No longer using a static StackTemplate; services are generated dynamically.

  /// Generates YAML for Watchtower service
  pub fn watchtower_service_yaml() -> &'static str {
      r#"  watchtower:
        mac_address: {{ mac_address }}
"#,
    ext = "yml"
)]
pub struct AppServiceTemplate<'a> {
    pub app_name: &'a str,
    pub mac_address: &'a str,
    pub domain_name: &'a str,
}

/// Data for the stack-level docker-compose template (includes app, Watchtower, Promtail)
  }

  /// Generates YAML for Promtail service
  pub fn promtail_service_yaml() -> &'static str {
      r#"  promtail:
#[derive(Template)]
#[template(
    source = r#"
version: '3.8'
services:
{{ app_service }}
  watchtower:
    image: containrrr/watchtower:latest
    container_name: watchtower
    environment:
      - DOCKER_API_VERSION=1.41
  }

  /// Generates YAML for Traefik service
  pub fn traefik_service_yaml() -> &'static str {
      r#"  traefik:
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    command: --cleanup --label-enable
    restart: unless-stopped
    labels:
      - "com.centurylinklabs.watchtower.enable=true"
  promtail:
    image: grafana/promtail:latest
    container_name: promtail
    command: -config.file=/etc/promtail/config.yml -config.expand-env=true
    volumes:
      - ./promtail/config.yml:/etc/promtail/config.yml:ro
      - /var/log:/var/log:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
    restart: unless-stopped
    labels:
  }

  /// Generate a full stack docker-compose.yml with selected default services
  pub fn scaffold_stack_with_services(app: &AppServiceTemplate, include_watchtower: bool, include_promtail: bool, include_traefik: bool) -> String {
      let mut services = String::new();
      // Main app service
      if let Ok(app_yaml) = scaffold_app(app) {
          services.push_str(&app_yaml);
      }
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
      format!(
          "version: '3.8'\nservices:\n{}networks:\n  default:\n    driver: bridge\n",
          services
      )
  }
      - "com.centurylinklabs.watchtower.enable=true"
networks:
  default:
    driver: bridge
"#,
    ext = "yml"
)]
pub struct StackTemplate {
    pub app_service: String,
}

/// Render only the app service YAML (for single-app compose or sub-apps)
pub fn scaffold_app(app: &AppServiceTemplate) -> Result<String, askama::Error> {
    app.render()
}

/// Render a full stack docker-compose.yml with app, Watchtower, and Promtail
pub fn scaffold_stack(app: &AppServiceTemplate) -> Result<String, askama::Error> {
    let app_service = scaffold_app(app)?;
    let stack = StackTemplate { app_service };
    stack.render()
}
