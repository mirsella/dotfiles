# Main Hotspot

Restore the `main hotspot` NetworkManager profile and its narrowly scoped
Docker forwarding rules on host `main`:

```bash
./apply-main-hotspot.sh
```

The script retrieves the WPA2 PSK from the `main hotspot` Wi-Fi item in the
Proton Pass `Personal` vault. The password is not stored in this repository.
It installs the root-owned files from `system/main/`, restarts Docker, and
activates the system-wide, pre-login NetworkManager hotspot profile. It aborts
unless the computer's short hostname is exactly `main`; normal `chezmoi apply`
never installs these files.
