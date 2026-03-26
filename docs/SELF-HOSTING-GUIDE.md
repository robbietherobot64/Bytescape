# Bytescape Self-Hosting Guide

> **Audience:** First-time self-hoster with basic terminal familiarity.
> **Hardware baseline:** Intel i5-4570 (Haswell, 4C/4T, 3.2 GHz), standard ATX/mATX motherboard, ATX PSU.
> **Goal:** Run the full Bytescape server stack (`bytescape.social`) from home hardware.

---

## Table of Contents

1. [What You're Building](#1-what-youre-building)
2. [Hardware Shopping List](#2-hardware-shopping-list)
3. [BIOS Setup](#3-bios-setup)
4. [Install Debian 12](#4-install-debian-12)
5. [First Boot — Secure the Server](#5-first-boot--secure-the-server)
6. [Install Docker](#6-install-docker)
7. [Domain & DNS Setup](#7-domain--dns-setup)
8. [Cloudflare Tunnel (No Port Forwarding)](#8-cloudflare-tunnel-no-port-forwarding)
9. [Deploy the Bytescape Stack](#9-deploy-the-bytescape-stack)
10. [Verify Everything Works](#10-verify-everything-works)
11. [Maintenance & Updates](#11-maintenance--updates)
12. [Troubleshooting](#12-troubleshooting)
13. [Glossary](#13-glossary)

---

## 1. What You're Building

A single home server running six services inside Docker containers:

```
Internet
  │
  ▼
Cloudflare Tunnel (encrypted, no open ports on your router)
  │
  ▼
┌─────────────────────────────────────────────────────────────┐
│  Your i5-4570 Server (Debian 12)                            │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │  Caddy    │  │  PDS     │  │  AP      │  │  Iroh      │  │
│  │  (reverse │  │  (AT     │  │  (fedi-  │  │  Relay     │  │
│  │   proxy)  │  │  Proto)  │  │  verse)  │  │  (P2P)     │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └─────┬──────┘  │
│       │              │             │               │         │
│  ┌────┴──────────────┴─────────────┴───────────────┴──────┐  │
│  │              Sync Bridge + PostgreSQL                   │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

**What each piece does:**

| Service | What it does | Why you need it |
|---------|-------------|-----------------|
| **Caddy** | Handles HTTPS, routes traffic to the right service | One front door for everything |
| **PDS** | AT Protocol Personal Data Server (Bluesky federation) | So you own your Bluesky identity |
| **AP Server** | ActivityPub instance (Mastodon/fediverse federation) | So you own your fediverse identity |
| **Iroh Relay** | Encrypted store-and-forward for Bytescape P2P sync | So your phone/laptop sync when your desktop is off |
| **Sync Bridge** | Translates between PDS ↔ AP ↔ Iroh ↔ PostgreSQL | The glue that makes it all work together |
| **PostgreSQL** | Database shared by PDS, AP server, and Sync Bridge | Where your data actually lives on disk |

**Total idle resource usage: ~1 GB RAM, ~3% CPU.** Your i5-4570 will be bored.

---

## 2. Hardware Shopping List

You already have: CPU + motherboard + PSU. Here's what else you need:

### Must Have

| Part | Spec | Why | Est. Cost (Used) |
|------|------|-----|-------------------|
| **RAM** | 8 GB DDR3-1600 minimum (16 GB recommended) | Postgres needs ~2 GB, Docker overhead ~500 MB, each Rust service ~50-100 MB. 8 GB works, 16 GB gives room for monitoring tools. | $10-15 |
| **SSD** | 120+ GB SATA SSD | Docker, Postgres, and the OS need fast random I/O. Do NOT use a spinning hard drive for the OS/database. Any brand works — Kingston, Crucial, Samsung, whatever's cheapest. | $10-15 |
| **Case** | Any mATX or ATX case with at least one fan mount | Haswell runs warm under sustained load. Doesn't need to be fancy — even an open test bench works for initial testing. | $15-25 |
| **Ethernet cable** | Cat5e or Cat6, long enough to reach your router | Wired connection is non-negotiable for a server. Wi-Fi drops will break federation. | $5-10 |

### Nice to Have

| Part | Spec | Why | Est. Cost |
|------|------|-----|-----------|
| **Data HDD** | 1+ TB SATA HDD | For media blob storage (imported photos/videos). Can add later — not needed for initial testing. | $15-25 |
| **USB flash drive** | 4+ GB | For the Debian installer. You probably have one in a drawer. | $0-5 |
| **UPS** | Basic 600VA | Protects against power blips corrupting your database. Optional but recommended for long-term use. | $40-60 |

### Total Estimated Cost: **$40-70** (without UPS)

### What You Do NOT Need
- ❌ GPU (there's no graphics processing happening)
- ❌ Wi-Fi card (use Ethernet)
- ❌ Sound card
- ❌ Optical drive
- ❌ A fancy case

---

## 3. BIOS Setup

After assembling the hardware, you need to configure a few BIOS settings. Press **F2** or **DEL** during boot to enter BIOS (varies by motherboard — watch the screen during POST).

### Required Settings

1. **Enable Virtualization (VT-x)**
   - Usually under `Advanced` → `CPU Configuration` → `Intel Virtualization Technology`
   - Set to **Enabled**
   - Docker needs this to run containers efficiently

2. **Set Boot Order**
   - Put your USB drive first (for installation)
   - Put your SSD second (for normal booting after install)

3. **Disable Secure Boot** (if present)
   - Some Haswell-era boards have this under `Security` or `Boot`
   - Debian can work with it enabled, but it adds complexity for no benefit on a home server

### Optional but Recommended

4. **Enable "Restore on AC Power Loss"**
   - Usually under `Power Management` or `Advanced`
   - Set to **Power On** (so the server automatically restarts after a power outage)
   - Names vary: "After Power Loss", "AC Back", "Restore AC Power Loss"

5. **Disable unnecessary onboard devices** (saves a tiny bit of power)
   - Onboard audio → Disabled
   - Serial/parallel ports → Disabled

Save and exit BIOS.

---

## 4. Install Debian 12

### 4.1 Create the Installer USB

On another computer, download the **Debian 12 netinst image** (small installer, downloads packages during install):

```
https://www.debian.org/download
```

Pick the **amd64** architecture. The file will be something like `debian-12.x.x-amd64-netinst.iso` (~600 MB).

**Flash it to your USB drive:**

- **Linux:** `sudo dd if=debian-12.x.x-amd64-netinst.iso of=/dev/sdX bs=4M status=progress` (replace `sdX` with your USB drive — **be careful, dd will overwrite everything**)
- **Windows:** Use [Rufus](https://rufus.ie/) or [balenaEtcher](https://etcher.balena.io/)
- **Mac:** Use [balenaEtcher](https://etcher.balena.io/)

### 4.2 Run the Installer

Boot from the USB drive. Select **Install** (not Graphical Install — text mode is fine and faster).

Walk through the prompts:

| Prompt | What to enter |
|--------|--------------|
| Language | English |
| Location | Your country |
| Keyboard | Your layout |
| Hostname | `bytescape` (or whatever you want — this is the machine's local name) |
| Domain name | Leave blank |
| Root password | Set a strong password and **write it down** |
| Full name | Your name |
| Username | `admin` (or your preferred username) |
| User password | Different from root password. **Write it down.** |

### 4.3 Disk Partitioning

When asked about partitioning:

1. Select **Guided - use entire disk**
2. Select your SSD
3. Select **All files in one partition** (simplest for a home server)
4. Select **Finish partitioning and write changes to disk**
5. Confirm **Yes**

### 4.4 Package Selection

When the package selector (tasksel) appears:

- ❌ **Uncheck** "Debian desktop environment" and "GNOME" (you don't want a GUI — it wastes RAM)
- ✅ **Check** "SSH server" (so you can manage the server remotely from your main computer)
- ✅ **Check** "Standard system utilities"

That's it. Let the installer finish and reboot. Remove the USB drive when prompted.

### 4.5 Find Your Server's IP Address

After reboot, log in at the physical console with your username and password, then run:

```bash
ip addr show
```

Look for something like `192.168.1.XXX` or `192.168.0.XXX` next to `inet` on the `enp0s25` or `eth0` interface. **Write this IP down** — you'll need it for SSH and for Cloudflare Tunnel later.

From now on, you can manage the server from your main computer via SSH:

```bash
ssh admin@192.168.1.XXX
```

---

## 5. First Boot — Secure the Server

All commands from here are run via SSH from your main computer (or at the server console).

### 5.1 Update Everything

```bash
sudo apt update && sudo apt upgrade -y
```

### 5.2 Set a Static IP (Recommended)

Your router might change the server's IP address on reboot. Fix it:

```bash
sudo nano /etc/network/interfaces
```

Find the line for your network interface (e.g., `iface enp0s25 inet dhcp`) and change it to:

```
auto enp0s25
iface enp0s25 inet static
    address 192.168.1.100
    netmask 255.255.255.0
    gateway 192.168.1.1
    dns-nameservers 1.1.1.1 8.8.8.8
```

> **Adjust these values:**
> - `address`: Pick an IP outside your router's DHCP range (check your router's admin page, usually `192.168.1.1` in a browser). Common safe picks: `.100`, `.150`, `.200`.
> - `gateway`: Your router's IP (usually `192.168.1.1` or `192.168.0.1`).
> - `enp0s25`: Your actual interface name from `ip addr show`.

Save (`Ctrl+O`, Enter, `Ctrl+X`) and restart networking:

```bash
sudo systemctl restart networking
```

Reconnect SSH using the new static IP if it changed.

### 5.3 Install Essential Tools

```bash
sudo apt install -y curl wget git htop nano ufw
```

### 5.4 Configure Firewall

```bash
sudo ufw allow ssh
sudo ufw enable
```

> **Note:** We're NOT opening ports 80/443 because Cloudflare Tunnel handles that. The only open port is SSH (22) for your management access.

### 5.5 (Optional) Restrict SSH to Key-Based Auth

This is a security best practice — it prevents password brute-force attacks.

On your **main computer** (not the server):

```bash
ssh-keygen -t ed25519 -C "bytescape-server"
ssh-copy-id admin@192.168.1.100
```

Then on the **server**, disable password login:

```bash
sudo nano /etc/ssh/sshd_config
```

Find and change these lines:
```
PasswordAuthentication no
PubkeyAuthentication yes
```

```bash
sudo systemctl restart sshd
```

> ⚠️ **Don't do this if you're not comfortable with SSH keys.** If you lose your key and password auth is disabled, you'll need physical access to the server to fix it.

---

## 6. Install Docker

Docker runs all the Bytescape services in isolated containers. This is the official installation method for Debian 12.

### 6.1 Add Docker's Repository

```bash
# Install prerequisites
sudo apt update
sudo apt install -y ca-certificates curl

# Add Docker's GPG key
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

# Add the Docker repository
sudo tee /etc/apt/sources.list.d/docker.sources <<EOF
Types: deb
URIs: https://download.docker.com/linux/debian
Suites: $(. /etc/os-release && echo "$VERSION_CODENAME")
Components: stable
Signed-By: /etc/apt/keyrings/docker.asc
EOF

sudo apt update
```

### 6.2 Install Docker Engine + Compose

```bash
sudo apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
```

### 6.3 Allow Your User to Run Docker (No Sudo)

```bash
sudo usermod -aG docker $USER
```

**Log out and back in** for this to take effect:

```bash
exit
ssh admin@192.168.1.100
```

### 6.4 Verify

```bash
docker run hello-world
```

You should see "Hello from Docker!" If so, Docker is working.

```bash
docker compose version
```

Should show something like `Docker Compose version v2.x.x`.

---

## 7. Domain & DNS Setup

You need a domain name that points to your server. For Bytescape, that's `bytescape.social`.

### 7.1 Buy a Domain (If You Haven't Already)

Recommended registrars:
- [Cloudflare Registrar](https://www.cloudflare.com/products/registrar/) — cheapest (at-cost pricing), and you'll use Cloudflare for DNS anyway
- [Namecheap](https://www.namecheap.com/)
- [Porkbun](https://porkbun.com/)

A `.social` domain typically costs $10-15/year.

### 7.2 Point DNS to Cloudflare

If you didn't buy the domain through Cloudflare, you need to add it to Cloudflare:

1. Create a free [Cloudflare account](https://dash.cloudflare.com/sign-up)
2. Click **"Add a site"** → enter `bytescape.social`
3. Select the **Free** plan
4. Cloudflare will give you two nameservers (e.g., `anna.ns.cloudflare.com`, `bob.ns.cloudflare.com`)
5. Go to your registrar and replace the existing nameservers with Cloudflare's
6. Wait for propagation (can take 5 minutes to 24 hours)

### 7.3 Configure DNS Records

In Cloudflare's DNS settings for `bytescape.social`, add these records:

| Type | Name | Content | Proxy | TTL |
|------|------|---------|-------|-----|
| A | `@` | `192.0.2.1` (placeholder — Cloudflare Tunnel overrides this) | Proxied ☁️ | Auto |
| A | `*` | `192.0.2.1` (same placeholder) | Proxied ☁️ | Auto |
| CNAME | `pds` | `bytescape.social` | Proxied ☁️ | Auto |
| CNAME | `relay` | `bytescape.social` | Proxied ☁️ | Auto |

> **Why placeholder IPs?** Cloudflare Tunnel routes traffic without needing your real IP. The `A` records are just placeholders that Cloudflare's proxy overrides. Some setups prefer CNAME-flattening — either way, the Tunnel takes precedence.

The **wildcard record** (`*`) is critical — it allows `alice.bytescape.social`, `bob.bytescape.social`, etc. to resolve for user handles.

> ⚠️ **Cloudflare wildcard proxying requires a paid plan ($20/month for Pro).** On the free plan, wildcard subdomains resolve but are NOT proxied (orange cloud off). This means wildcard subdomains would need direct IP access, which defeats the purpose of the tunnel for handles. **Workaround:** Use path-based handles (`bytescape.social/@alice`) instead of subdomain handles on the free plan, or add individual CNAME records for each user (`alice.bytescape.social`).

---

## 8. Cloudflare Tunnel (No Port Forwarding)

This is the magic that lets you run a public server from home **without opening any ports on your router** and **without exposing your home IP address**.

### 8.1 What Is a Cloudflare Tunnel?

Normally, to host a website from home, you'd need to:
1. Open ports 80 and 443 on your router (security risk)
2. Give out your home IP address (privacy risk)
3. Deal with your ISP potentially blocking ports or changing your IP

A Cloudflare Tunnel flips this: your server makes an **outbound** connection to Cloudflare, and Cloudflare routes incoming web traffic through that connection. No open ports, no exposed IP.

### 8.2 Install cloudflared

On your server:

```bash
# Download and install cloudflared
curl -L --output cloudflared.deb https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb
sudo dpkg -i cloudflared.deb
rm cloudflared.deb
```

### 8.3 Authenticate

```bash
cloudflared tunnel login
```

This prints a URL. Open it in your browser, select `bytescape.social`, and authorize. This creates a certificate at `~/.cloudflared/cert.pem`.

### 8.4 Create the Tunnel

```bash
cloudflared tunnel create bytescape
```

This outputs a **Tunnel ID** (a UUID like `a1b2c3d4-e5f6-...`). **Save this** — you'll need it.

It also creates a credentials file at `~/.cloudflared/<TUNNEL_ID>.json`.

### 8.5 Configure the Tunnel

Create the config file:

```bash
mkdir -p ~/.cloudflared
nano ~/.cloudflared/config.yml
```

Paste this (replace `YOUR_TUNNEL_ID` with the actual UUID):

```yaml
tunnel: YOUR_TUNNEL_ID
credentials-file: /home/admin/.cloudflared/YOUR_TUNNEL_ID.json

ingress:
  # AT Protocol PDS (XRPC endpoints)
  - hostname: pds.bytescape.social
    service: http://localhost:3000

  # Main domain and AP server
  - hostname: bytescape.social
    service: http://localhost:8080

  # Wildcard for user handle subdomains
  - hostname: "*.bytescape.social"
    service: http://localhost:8080

  # Catch-all (required by cloudflared)
  - service: http_status:404
```

> **Port mapping:** Caddy (inside Docker) will listen on `localhost:8080` and `localhost:3000` and route to the appropriate services. These ports are only accessible locally — Cloudflare Tunnel is the only way in from the internet.

### 8.6 Route DNS Through the Tunnel

```bash
cloudflared tunnel route dns bytescape bytescape.social
cloudflared tunnel route dns bytescape "*.bytescape.social"
cloudflared tunnel route dns bytescape pds.bytescape.social
```

This automatically creates the right CNAME records in Cloudflare DNS pointing to your tunnel. These override the placeholder A records from Step 7.3.

### 8.7 Run the Tunnel as a System Service

```bash
sudo cloudflared service install
sudo systemctl enable cloudflared
sudo systemctl start cloudflared
```

Verify it's running:

```bash
sudo systemctl status cloudflared
```

You should see `active (running)`. The tunnel is now live and will auto-start on boot.

### 8.8 Verify the Tunnel in Cloudflare Dashboard

1. Go to [Cloudflare One](https://one.dash.cloudflare.com/) → Networks → Connectors → Cloudflare Tunnels
2. You should see your `bytescape` tunnel with a **Healthy** status

---

## 9. Deploy the Bytescape Stack

> ⚠️ **This section is a preview.** The Docker Compose file and custom services (AP server, Sync Bridge, Iroh Relay) are still being built by the development team. The structure below shows what the final deployment will look like, with the **Official PDS** as the AT Protocol component that can be deployed today.

### 9.1 Create the Project Directory

```bash
sudo mkdir -p /opt/bytescape
sudo chown $USER:$USER /opt/bytescape
cd /opt/bytescape
```

### 9.2 The Docker Compose File

Create `/opt/bytescape/docker-compose.yml`:

```yaml
# Bytescape Server Stack
# This file will be provided by the project repository.
# Below is the target architecture.

services:
  # ─── Reverse Proxy ─────────────────────────────────────
  caddy:
    image: caddy:2-alpine
    restart: unless-stopped
    ports:
      - "127.0.0.1:8080:80"     # Cloudflare Tunnel connects here
      - "127.0.0.1:8443:443"    # Internal HTTPS (optional)
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy_data:/data
      - caddy_config:/config
    depends_on:
      - pds
      - postgres

  # ─── AT Protocol PDS ───────────────────────────────────
  pds:
    image: ghcr.io/bluesky-social/pds:latest
    restart: unless-stopped
    volumes:
      - pds_data:/pds
    env_file:
      - ./pds.env
    depends_on:
      - postgres

  # ─── ActivityPub Server ────────────────────────────────
  # (Custom axum server — image TBD, will be built from source)
  # ap-server:
  #   image: bytescape/ap-server:latest
  #   restart: unless-stopped
  #   env_file:
  #     - ./ap.env
  #   depends_on:
  #     - postgres

  # ─── Iroh Relay ────────────────────────────────────────
  # (Custom Rust relay — image TBD)
  # iroh-relay:
  #   image: bytescape/iroh-relay:latest
  #   restart: unless-stopped
  #   env_file:
  #     - ./relay.env

  # ─── Sync Bridge ───────────────────────────────────────
  # (Custom Rust daemon — image TBD)
  # sync-bridge:
  #   image: bytescape/sync-bridge:latest
  #   restart: unless-stopped
  #   env_file:
  #     - ./bridge.env
  #   depends_on:
  #     - pds
  #     - postgres

  # ─── PostgreSQL ────────────────────────────────────────
  postgres:
    image: postgres:16-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: bytescape
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: bytescape
    volumes:
      - postgres_data:/var/lib/postgresql/data
    # Only accessible from other containers, not the host network
    expose:
      - "5432"

volumes:
  caddy_data:
  caddy_config:
  pds_data:
  postgres_data:
```

### 9.3 Environment File

Create `/opt/bytescape/.env` with generated secrets:

```bash
# Generate secrets
POSTGRES_PASSWORD=$(openssl rand -hex 32)
PDS_JWT_SECRET=$(openssl rand -hex 16)
PDS_ADMIN_PASSWORD=$(openssl rand -hex 16)

cat > /opt/bytescape/.env << EOF
POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
PDS_JWT_SECRET=${PDS_JWT_SECRET}
PDS_ADMIN_PASSWORD=${PDS_ADMIN_PASSWORD}
EOF

# Lock it down
chmod 600 /opt/bytescape/.env

echo "Secrets generated. SAVE THESE SOMEWHERE SAFE:"
echo "  Postgres password: ${POSTGRES_PASSWORD}"
echo "  PDS JWT secret:    ${PDS_JWT_SECRET}"
echo "  PDS admin password: ${PDS_ADMIN_PASSWORD}"
```

> ⚠️ **Write these down and store them securely.** If you lose the PDS admin password, you lose admin access. If you lose the JWT secret, all sessions are invalidated.

### 9.4 Deploy What's Available Now

For now, you can deploy the **Official PDS** standalone using Bluesky's own installer, which sets up its own Caddy + PDS + Docker Compose:

```bash
curl https://raw.githubusercontent.com/bluesky-social/pds/main/installer.sh > installer.sh
sudo bash installer.sh
```

The installer will ask for:
- Your domain name (`bytescape.social`)
- An admin email
- Whether to create an account

This gives you a working Bluesky PDS immediately. The other services (AP server, Iroh relay, Sync Bridge) will be added to the Compose file as they're built.

### 9.5 Start the Full Stack (When Ready)

Once all services are built:

```bash
cd /opt/bytescape
docker compose up -d
```

Check status:

```bash
docker compose ps
docker compose logs -f   # Live log tail (Ctrl+C to exit)
```

---

## 10. Verify Everything Works

### 10.1 PDS Health Check

From your main computer (or any browser):

```
https://bytescape.social/xrpc/_health
```

You should see:
```json
{"version":"0.4.x"}
```

### 10.2 WebFinger (AP Federation)

```
https://bytescape.social/.well-known/webfinger?resource=acct:admin@bytescape.social
```

Should return a JSON document with your actor information (once the AP server is running).

### 10.3 AT Protocol Handle Resolution

```
https://bytescape.social/.well-known/atproto-did
```

Should return your DID.

### 10.4 Check from Inside the Server

```bash
# Are all containers running?
docker compose ps

# Check resource usage
docker stats --no-stream

# Check disk usage
df -h
```

---

## 11. Maintenance & Updates

### Daily (Automatic)

Nothing — Docker containers auto-restart on failure via `restart: unless-stopped`.

### Weekly (2 minutes)

```bash
# Check disk space
df -h

# Check for Debian security updates
sudo apt update && sudo apt upgrade -y

# Check container logs for errors
cd /opt/bytescape
docker compose logs --since 7d | grep -i error
```

### Monthly (5 minutes)

```bash
# Update Docker images
cd /opt/bytescape
docker compose pull
docker compose up -d

# Clean old images
docker image prune -f
```

### PDS Updates

The official PDS has its own update mechanism:

```bash
sudo pdsadmin update
```

### Backups

Your data lives in Docker volumes. Back up the important ones:

```bash
# Stop services briefly for a clean backup
cd /opt/bytescape
docker compose stop postgres

# Backup Postgres data
sudo tar czf /tmp/bytescape-backup-$(date +%Y%m%d).tar.gz \
  -C /var/lib/docker/volumes/ \
  bytescape_postgres_data \
  bytescape_pds_data

docker compose start postgres

# Copy backup somewhere safe (external drive, another computer, cloud)
```

> **How often?** Weekly for personal use. Daily if you have other users' data.

---

## 12. Troubleshooting

### "I can't SSH into my server"

1. Is the server powered on? (Check physically)
2. Is the Ethernet cable plugged in?
3. Did the IP change? Check your router's admin page for connected devices
4. Did you lock yourself out with SSH key settings? You'll need physical access + keyboard/monitor

### "Cloudflare Tunnel shows Inactive/Down"

```bash
sudo systemctl status cloudflared    # Check the service
sudo journalctl -u cloudflared -n 50 # Check recent logs
```

Common fixes:
- Restart: `sudo systemctl restart cloudflared`
- Re-authenticate: `cloudflared tunnel login`
- Check internet: `ping 1.1.1.1`

### "PDS health check returns an error"

```bash
docker compose logs pds --tail 100   # Check PDS logs
docker compose restart pds           # Try restarting
```

### "Database connection error"

```bash
docker compose logs postgres --tail 50
docker compose restart postgres
```

If Postgres won't start, it may be out of disk space:
```bash
df -h
docker system prune -f   # Remove unused Docker data
```

### "Everything was working and now nothing is"

Most likely causes:
1. **Power outage** → Server should auto-restart (BIOS setting from Step 3). Check `uptime`.
2. **ISP changed your IP** → Cloudflare Tunnel doesn't care about this. Check `sudo systemctl status cloudflared`.
3. **Disk full** → `df -h`, then `docker system prune` and check Postgres vacuuming.
4. **Debian updated something** → Check `journalctl -b` for boot errors.

### Nuclear Option (If All Else Fails)

```bash
cd /opt/bytescape
docker compose down
docker compose up -d
```

This stops everything and restarts it cleanly. Data is preserved in Docker volumes.

---

## 13. Glossary

| Term | What It Means |
|------|--------------|
| **PDS** | Personal Data Server — stores your AT Protocol (Bluesky) data and serves it to the network |
| **ActivityPub (AP)** | The protocol that Mastodon, Pixelfed, Lemmy, etc. use to talk to each other |
| **Federation** | Servers talking to each other to share posts/follows/messages |
| **Docker** | Runs applications in isolated containers, so they don't conflict with each other or your OS |
| **Docker Compose** | Manages multiple Docker containers as a single unit |
| **Caddy** | A web server that automatically handles HTTPS certificates |
| **Cloudflare Tunnel** | A secure connection from your server to Cloudflare that lets you host websites without opening firewall ports |
| **DID** | Decentralized Identifier — a persistent identity string (like `did:web:bytescape.social:alice`) |
| **XRPC** | The API protocol AT Protocol uses (similar to REST APIs) |
| **WebFinger** | A protocol that lets other servers look up user information by their `@user@domain` handle |
| **SSH** | Secure Shell — the encrypted protocol for remotely managing your server via command line |
| **SSD** | Solid State Drive — fast storage with no moving parts |
| **HDD** | Hard Disk Drive — slower storage with spinning platters, good for bulk storage |
| **VT-x** | Intel's hardware virtualization technology — needed for Docker |
| **DHCP** | Your router's system for automatically assigning IP addresses to devices |
| **Static IP** | An IP address that doesn't change (assigned manually instead of by DHCP) |
| **TTL** | Time To Live — how long a DNS record is cached before re-checking (in seconds) |
| **CNAME** | A DNS record type that aliases one domain name to another |
| **UFW** | Uncomplicated Firewall — a simple firewall manager for Linux |

---

## Appendix A: Quick Reference Card

Once everything is set up, these are the commands you'll use most:

```bash
# SSH into your server
ssh admin@192.168.1.100

# Check if everything is running
cd /opt/bytescape && docker compose ps

# View live logs
docker compose logs -f

# Restart everything
docker compose restart

# Update containers
docker compose pull && docker compose up -d

# Check disk/memory/CPU
htop                    # Interactive (q to quit)
df -h                   # Disk space
free -h                 # Memory

# Check tunnel status
sudo systemctl status cloudflared

# Update Debian
sudo apt update && sudo apt upgrade -y
```

---

## Appendix B: What's Deployable Now vs. Later

| Component | Status | When |
|-----------|--------|------|
| **Debian 12 + Docker** | ✅ Deploy now | Today |
| **Cloudflare Tunnel** | ✅ Deploy now | Today |
| **Official Bluesky PDS** | ✅ Deploy now | Today (use their installer) |
| **PostgreSQL** | ✅ Deploy now | Today (part of Compose stack) |
| **Caddy** | ✅ Deploy now | Today (part of Compose stack) |
| **Custom AP Server** | 🔨 In development | Phase 4.5 |
| **Iroh Relay** | 🔨 In development | Phase 4.5 |
| **Sync Bridge** | 🔨 In development | Phase 4.5 |
| **Custom Domain Handles** | 🔨 In development | Phase 4.5 |

**You can start Steps 1-8 right now** and have a working PDS federating with Bluesky within an afternoon. The rest gets added incrementally as the development team builds it.

---

*Last updated: March 25, 2026*
*Hardware baseline: Intel i5-4570 / DDR3 / SATA SSD / Cloudflare Tunnel*
