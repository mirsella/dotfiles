# System Backup

Refresh the current host's root configuration snapshot:

```bash
./update-system-backup.sh
```

The script stores the snapshot under `system/$(hostname -s)/`. These files are
for manual recovery and are never installed by `chezmoi apply`.
