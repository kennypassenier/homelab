//! Scaffolding logic and docker-compose template rendering for Homelab Client.
use askama::Template;
use rand::prelude::SliceRandom;
use rand::{Rng, thread_rng};

/// Generates a random Locally Administered MAC address (first octet ends in 2, 6, A, or E).
pub fn generate_mac_address() -> String {
    let mut rng = thread_rng();
    let first_octet_choices = [0x02, 0x06, 0x0A, 0x0E];
    let first = *first_octet_choices.choose(&mut rng).unwrap();
    let mac: Vec<String> = std::iter::once(first)
        .chain((0..5).map(|_| rng.gen_range(0x00..=0xFF)))
        .map(|b| format!("{:02X}", b))
        .collect();
    mac.join(":")
}

/// Data for the app service in docker-compose.
#[derive(Template)]
#[template(
    source = r#"
  {{ app_name }}:
    image: your-image:latest
    container_name: {{ app_name }}
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.{{ app_name }}.rule=Host(\"{{ domain_name }}\")"
      - "com.centurylinklabs.watchtower.enable=true"
      - "com.homelab.backup.pause=true"
    volumes:
      - ../{{ app_name }}-config:/config
    networks:
      default:
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
