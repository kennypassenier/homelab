# Fase 1: De CLIENT App (Tier 1) - Implementatie Prompts

**Doel:** Een veilige, lokaal draaiende Ratatui TUI ter vervanging van bash scripts. Deze app draait NOOIT op de Proxmox server, enforceert "Git = God", en levert pre-flight linting plus live SSE telemetrie.

**Globale Codeerrichtlijnen voor alle prompts:**
* All generated code must be in English.
* Add thorough comments explaining the logic.
* Keep a structured and clear naming convention.
* Write atomic, modular, and robust code. Do not add comments stating "this is robust" or "this is atomic", just implement it that way.
* Do not use any emojis in the UI or in the code comments.

---

## Stap 1: Project Initialization & Core TUI Setup
**Copilot Prompt:**
`I am building a new Rust CLI application for homelab management called "client-app".
Task 1: Initialize the cargo project and add the following dependencies: ratatui, crossterm, tokio (with full features), anyhow, and color-eyre.
Task 2: Write the main entry point (main.rs) that sets up a basic Ratatui terminal backend using crossterm. 
Task 3: Implement a panic hook (using color-eyre or standard panic handlers) that guarantees disable_raw_mode() is called and the terminal state is fully restored if the application crashes.
Task 4: Implement the base asynchronous run-loop that draws a blank screen with the text "Homelab Client - Press 'q' to exit" and handles the 'q' keypress to gracefully exit.`

## Stap 2: UI Framework, Theming & Navigation
**Copilot Prompt:**
`Extend the Ratatui application with a premium UI framework and State Machine for navigation.
Task 1: Implement a centralized styling module defining a cyberpunk color palette (e.g., Cyan/Magenta accents, dark grey backgrounds).
Task 2: Create an App struct holding the current state. Add an enum Tab with variants: Dashboard, Scaffolding, and HostManagement.
Task 3: Update the drawing function to split the screen vertically: a Top Bar (size 3) and a Main Content area. All layout blocks must use `BorderType::Rounded` with appropriate padding.
Task 4: In the Top Bar, render a Tabs widget highlighting the active tab using the custom color palette.
Task 5: Update the event loop to cycle through the Tab enum using the Left and Right arrow keys, and display the active tab's name in the Main Content area.`

## Stap 3: Domain Logic - Scaffolding & Templating
**Copilot Prompt:**
`Implement the application scaffolding logic using templates.
Task 1: Add the askama (or tera) and rand crates to Cargo.toml.
Task 2: Create a modular function generate_mac_address() -> String that generates a random Locally Administered MAC address (the first octet must end in 2, 6, A, or E).
Task 3: Define a struct AppTemplate with fields: app_name, mac_address, and domain_name.
Task 4: Write a template for a docker-compose.yml file. It must include:
- Traefik router label: traefik.http.routers.{{app_name}}.rule=Host("{{domain_name}}")
- Traefik enable label: traefik.enable=true
- Watchtower label: com.centurylinklabs.watchtower.enable=true
- Backup label: com.homelab.backup.pause=true
- The generated mac_address field on the network configuration.
Task 5: Write a function scaffold_app(template: AppTemplate) that renders this template to a String.`

## Stap 4: Domain Logic - Native GitOps & Linting
**Copilot Prompt:**
`Implement GitOps pushing and pre-flight validation.
Task 1: Add the git2 and serde_yaml crates to Cargo.toml.
Task 2: Create a function pre_flight_check(file_path: &str) -> Result<(), anyhow::Error>. It must parse the generated docker-compose.yml using serde_yaml and return an Error if the YAML is invalid.
Task 3: Create a function commit_and_push(repo_path: &str, commit_message: &str) -> Result<(), anyhow::Error>. 
Task 4: Using git2, this function must open the local repository, stage all modified/new files, create a commit, and push to the origin remote on the main branch. Handle Git credentials cleanly assuming an SSH agent is running.`

## Stap 5: Telemetry - HTTP Push API & SSE Streams
**Copilot Prompt:**
`Implement HTTP triggering and live Server-Sent Events (SSE) log streaming in the TUI.
Task 1: Add reqwest (with stream features) and reqwest-eventsource to Cargo.toml.
Task 2: Update the App state to include a thread-safe Arc<Mutex<Vec<String>>> to store incoming log lines.
Task 3: Create an async function trigger_deployment(api_url: &str). It should send an HTTP POST request with a Bearer token.
Task 4: Immediately after the POST, establish an SSE connection using reqwest-eventsource. Append incoming event text to the thread-safe Vec<String>.
Task 5: Update the TUI rendering loop. Add a Paragraph widget at the bottom of the screen (height: 10) that reads from the Vec<String> and displays the latest log lines. Ensure UI responsiveness.`

## Stap 6: Interactive UI - Blast Radius Protection
**Copilot Prompt:**
`Implement a strict "Blast Radius" security feature for destructive actions.
Task 1: Add the tui-input crate to Cargo.toml.
Task 2: Add an ActiveModal enum to the App state to track warning modals, including a delete_confirmation_input field.
Task 3: Create a drawing function draw_warning_modal. Use the Clear widget to overlay a centered, fixed-size popup over the main UI. Render a darkened background layer beneath it to create a depth effect.
Task 4: Style the modal with a stark Red border and warning text: "DANGER: Type the exact name of the app to delete it." Render the tui-input field inside.
Task 5: Update the event loop. If ActiveModal is open, route all keyboard inputs to the tui-input handler. On Enter, verify if the input matches the target app name. If successful, proceed and close the modal. If it fails, clear the input. Escape closes the modal safely.`

## Stap 7: Documentation Generation
**Copilot Prompt 1 (Human User Wiki):**
`The code for the Rust Client TUI is complete. Based strictly on the implemented code, write a comprehensive markdown file named docs/wiki/client-app.md. Explain how to launch and navigate the TUI, document all features (Dashboard, Scaffolding, Traefik, Blast Radius modal, GitOps workflow), and provide examples of the real-time push mechanism.`

**Copilot Prompt 2 (AI Context File):**
`Write a markdown file named docs/LLM_CONTEXT_CLIENT.md targeted at future AI Agents. Explicitly state the "Core Principles": NEVER runs on Proxmox, Git=God (no SSH execution for deployments), and the Blast Radius validation. List the core Rust crates used and detail the architectural flow of the HTTP Push API and SSE telemetry mechanism.`
## Stap 1.8: Legacy client.sh Feature Parity
**Copilot Prompt:**
`We need to ensure full feature parity with our old bash-based 'client.sh'. 
Task 1: Extend the Scaffolding module to specifically handle the "Create new Stack" and "Create new App" workflows. It must dynamically generate docker-compose.yml templates that automatically include our standard Watchtower and Promtail boilerplate (as described in our GitOps specs).
Task 2: Ensure the HostManagement tab includes the logic for the old 'add-ssh.sh' script. Create a function that parses the local ~/.ssh/config and idempotently adds or updates an SSH alias with a new LXC container's IP address, without corrupting existing entries.
Task 3: Ensure the Blast Radius module covers both 'Remove App' and 'Remove Stack', triggering the correct Git deletion and commit sequence before pushing.`
