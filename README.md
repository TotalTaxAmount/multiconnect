# Multiconnect
---
## This project is in its infancy and does not function yet
Sync Android and Linux (maybe Windows?) devices

## How to build/run
First clone the repo then cd into it.
### Desktop
You can use `MC_LOG={trace|debug|info|warn|error}` to change the loglevel.
You can also specify the config directory with `MC_CONFIG_DIR=<dir>`. These work on both the client and the daemon.
<br />
<br />
**Daemon**
<br />
You can specifiy which port to run the daemon on (for the local TCP socket not the p2p port) with `MC_PORT=<port>`
```bash
$ cd multiconnect-daemon
$ cargo run dev
```

**Client**
<br />
You can specific what port the daemon you want to connect to is runnig on with `MCD_PORT=<port>`
```
$ cargo tauri dev
```
