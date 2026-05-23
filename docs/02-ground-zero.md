# Phase 1: Ground Zero (Local Setup)

Before you can deploy any applications to your Proxmox server, you must establish a secure foundation on your local workstation (e.g., Linux desktop). In this GitOps architecture, all configuration is public by default, except for sensitive data (like passwords, API keys, and database credentials). 

"Ground Zero" previously initialized repository encryption using SOPS and Age. This is now deprecated. No action required.

---

## 1. The Goal of Ground Zero

Ground Zero is now deprecated. Secrets are managed via local uncommitted `.env` files. No encryption setup is required.

## 2. Running the Initialization

No initialization is required. SOPS/Age encryption is no longer used. Simply create your `.env` files locally and do not commit them to version control.

## 3. Commit the Base Infrastructure

No special commit or encryption setup is required for secrets. Do not add `.env` files to version control.

## 4. How the Servers Use This

Secrets are now managed via local `.env` files. No decryption or passphrase is required on the server.

---

**Next Steps:**
Now that your local workstation is securely configured and the base infrastructure is pushed to Git, you are ready to provision your first server. 

Proceed to **[Part 3: Bootstrapping a Host & Network Config](03-bootstrapping-and-networking.md)**.