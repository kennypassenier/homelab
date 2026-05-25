# Fase 1: De CLIENT App (Tier 1)
**Doel:** Een veilige, lokaal draaiende Ratatui TUI ter vervanging van bash scripts. Deze app draait NOOIT op de Proxmox server, enforceert "Git = God", en levert pre-flight linting plus live SSE telemetrie.

## Stap 1.1: Project Set-up & Core Dependencies
**Copilot Prompt:**
`I am building a new Rust CLI application for homelab management. 
Task 1: Initialize a new cargo project. 
Task 2: Add the following dependencies to Cargo.toml: ratatui, crossterm, tokio (with full features), anyhow, and color-eyre.
Task 3: Write a main.rs that sets up a basic Ratatui terminal backend using crossterm. 
CRITICAL: You MUST implement a robust panic hook (using color-eyre or standard panic handlers) that guarantees disable_raw_mode() is called and the terminal state is fully restored if the application crashes or exits unexpectedly. 
Task 4: Implement a basic run-loop that draws a blank screen with the text "Homelab Client - Press 'q' to exit" and handles the 'q' keypress to gracefully exit. Keep it in a single file for now.`

## Stap 1.2: State Machine & Navigatie
**Copilot Prompt:**
`Extend our Ratatui application with a State Machine for navigation.
Task 1: Create an App struct that holds the current state. Add an enum Tab with variants: Dashboard, Scaffolding, and HostManagement.
Task 2: Update the drawing function to split the screen vertically into two layout chunks: a Top Bar (size 3) and a Main Content area (Min 0).
Task 3: In the Top Bar, render a Tabs widget that highlights the currently active tab based on the App state. Use a premium visual style (e.g., cyan for the active tab, borders).
Task 4: Update the event loop so that pressing the Left and Right arrow keys cycles through the Tab enum, and the Main Content area displays a dummy paragraph indicating which tab is currently active.`

## Stap 1.3: Scaffolding & Traefik Templating
**Copilot Prompt:**
`We need to implement the scaffolding logic in our Rust app.
Task 1: Add the askama (or tera) template crate and the rand crate to Cargo.toml.
Task 2: Create a function generate_mac_address() -> String that generates a random Locally Administered MAC address. The first octet MUST end in 2, 6, A, or E (e.g., starting with 02:). Return it as a formatted string.
Task 3: Create a struct AppTemplate that holds fields: app_name, mac_address, and domain_name.
Task 4: Write a template for a docker-compose.yml file. It MUST include:
- A Traefik router label: traefik.http.routers.{{app_name}}.rule=Host("{{domain_name}}")
- A Traefik enable label: traefik.enable=true
- A Watchtower label: com.centurylinklabs.watchtower.enable=true
- A Backup label: com.homelab.backup.pause=true
- A mac_address field on the network configuration.
Task 5: Write a function scaffold_app(template: AppTemplate) that renders this template to a String.`

## Stap 1.4: De "Blast Radius" Beveiliging (Destructieve Acties)
**Copilot Prompt:**
`We need to implement a strict "Blast Radius" security feature for destructive actions (like deleting an app).
Task 1: Add the tui-input crate to Cargo.toml to handle user text input.
Task 2: Add an ActiveModal enum to the App state to track if a warning modal is open. Add a delete_confirmation_input field using tui-input.
Task 3: Create a rendering function draw_warning_modal. It must use Clear to overlay a centered, fixed-size popup over the main UI. 
Task 4: The modal MUST have a stark Red border and warning text saying "DANGER: Type the exact name of the app to delete it." Render the tui-input field inside this modal.
Task 5: In the event loop, if ActiveModal is open, route all keyboard inputs to the tui-input handler. If the user presses Enter, check if the input string exactly matches the target app name. If it matches, print a success log and close the modal. If it fails, clear the input. Escape should close the modal safely.`

## Stap 1.5: Native GitOps & Pre-flight Linting
**Copilot Prompt:**
`We need to implement GitOps pushing and pre-flight linting.
Task 1: Add the git2 and serde_yaml crates to Cargo.toml.
Task 2: Create a function pre_flight_check(file_path: &str) -> Result<(), anyhow::Error>. It should open the generated docker-compose.yml, parse it using serde_yaml, and return an Error if the YAML is invalid. This guarantees we never commit broken configurations.
Task 3: Create a function commit_and_push(repo_path: &str, commit_message: &str) -> Result<(), anyhow::Error>. 
Task 4: Using git2, this function must: 
- Open the local repository.
- Stage all modified and new files.
- Create a commit with the provided commit_message.
- Push the commit to the origin remote on the main branch. 
Handle potential Git credentials cleanly (assume SSH agent is running).`

## Stap 1.6: HTTP Push API & SSE Stream Viewer
**Copilot Prompt:**
`We need to implement an HTTP trigger and render a live Server-Sent Events (SSE) log stream in our TUI.
Task 1: Add reqwest (with stream features) and reqwest-eventsource to Cargo.toml.
Task 2: In the App state, add a thread-safe Arc<Mutex<Vec<String>>> to hold incoming log lines.
Task 3: Create an async function trigger_deployment(api_url: &str, force_delete: bool). Send an HTTP POST request to the api_url with a Bearer token.
Task 4: After the POST, establish an SSE connection using reqwest-eventsource. As events stream in, append the event text to the thread-safe Vec<String>. 
Task 5: In the TUI rendering loop, create a new Paragraph widget at the bottom of the screen (height: 10) that reads from the Vec<String> and displays the latest 10 log lines. Ensure the UI remains responsive.`

## Stap 1.7: Genereer Documentatie (Voor Copilot als de code af is)
**Prompt 1 (Menselijke Wiki):**
`The code for the Rust Client TUI is complete. Based on the implementation, write a comprehensive markdown file named docs/wiki/client-app.md. Explain to the human user how to launch and navigate the TUI, document every feature (Dashboard, Scaffolding, Traefik, Blast Radius modal, GitOps workflow), and provide examples of the real-time push mechanism.`

**Prompt 2 (LLM Context):**
`Write a markdown file named docs/LLM_CONTEXT_CLIENT.md targeted at AI Agents. Explicitly state the "Core Principles": NEVER runs on Proxmox, Git=God (no SSH execution for deployments), and the UI Blast Radius validation. List the core Rust crates used and detail the HTTP Push API / SSE telemetry mechanism.`