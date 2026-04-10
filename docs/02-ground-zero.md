# Phase 1: Ground Zero (Local Setup)

Before you can deploy any applications to your Proxmox server, you must establish a secure foundation on your local workstation (e.g., Linux desktop). In this GitOps architecture, all configuration is public by default, except for sensitive data (like passwords, API keys, and database credentials). 

"Ground Zero" is the process of initializing transparent repository encryption using **Mozilla SOPS** and **Age**.

---

## 1. The Goal of Ground Zero

By completing this phase, your local Git repository will be configured with specialized hooks (smudge and clean filters). 
These hooks ensure that every time you stage and commit a `.env` file, Git automatically intercepts the file and encrypts its contents before saving it to the repository history. When you pull or checkout files locally, Git automatically decrypts them so you can edit them in plaintext.

This means:
- You never accidentally push a plaintext password to GitHub.
- You never have to manually run encryption commands before committing.
- The workflow remains identical to a standard, non-encrypted Git workflow.

## 2. Running the Initialization

To set up your local machine, we will use the interactive client manager.

1. Open your terminal and navigate to the root of the `homelab` repository.
2. Execute the client manager script:
   ```bash
   ./client.sh
   ```
3. Select the option: **`6. Initialize Ground Zero (SOPS Encryption Setup)`**

### What the script does behind the scenes:
- **Key Generation:** It generates a new cryptographic Age keypair (public and private key).
- **Configuration:** It creates a `.sops.yaml` file at the root of the repository. This file tells SOPS to use your newly generated public Age key for all future encryption operations.
- **Git Attributes:** It generates a `.gitattributes` file that instructs Git to pass all `.env` files through the SOPS encryption filters.
- **Symmetric Encryption:** It will prompt you to enter a **strong passphrase**. The script uses this passphrase to symmetrically encrypt your *private* Age key. The resulting encrypted file is saved as `secrets/age.key.enc`.

> **⚠️ CRITICAL WARNING:** 
> The passphrase you enter during this step is the master key to your entire kingdom. If you lose this passphrase, you will permanently lose the ability to decrypt your repository's `.env` files, and your servers will not be able to bootstrap. **Save this passphrase immediately in a secure password manager like Bitwarden.**

## 3. Commit the Base Infrastructure

Once the `client.sh` script completes successfully, your local repository is armed and ready. However, the rest of the GitOps pipeline (and the servers you will build later) needs access to the public configuration and the encrypted private key.

You must commit these foundational files to your `main` branch. Run the following commands:

```bash
# Add the newly generated configuration and encrypted secret
git add .sops.yaml .gitattributes secrets/age.key.enc scripts/

# Commit the setup to the repository
git commit -m "chore: Initialize Ground Zero encryption"

# Push to your remote repository (e.g., GitHub)
git push -u origin main
```

## 4. How the Servers Use This

You might wonder how the Proxmox LXC containers are able to read these encrypted `.env` files if the private key is also encrypted (`secrets/age.key.enc`). 

Later, when you run the host bootstrap script to create a new LXC container, the script will ask you for the very same **strong passphrase** you created in Step 2. The server uses that passphrase to unlock `secrets/age.key.enc` locally in memory, retrieving the private Age key. From that moment on, the 5-minute GitOps cronjob inside the container can seamlessly decrypt the `.env` files just like your local machine does.

---

**Next Steps:**
Now that your local workstation is securely configured and the base infrastructure is pushed to Git, you are ready to provision your first server. 

Proceed to **[Part 3: Bootstrapping a Host & Network Config](03-bootstrapping-and-networking.md)**.