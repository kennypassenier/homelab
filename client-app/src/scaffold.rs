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

/// Data for the docker-compose template.
#[derive(Template)]
#[template(
    source = r#"
version: '3.8'
services:
  {{ app_name }}:
    image: your-image:latest
    container_name: {{ app_name }}
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.{{ app_name }}.rule=Host(\"{{ domain_name }}\")"
      - "com.centurylinklabs.watchtower.enable=true"
      - "com.homelab.backup.pause=true"
    networks:
      default:
        mac_address: {{ mac_address }}
networks:
  default:
    driver: bridge
"#,
    ext = "yml"
)]
pub struct AppTemplate<'a> {
    pub app_name: &'a str,
    pub mac_address: &'a str,
    pub domain_name: &'a str,
}

/// Renders the docker-compose template to a String.
pub fn scaffold_app(template: &AppTemplate) -> Result<String, askama::Error> {
    template.render()
}
