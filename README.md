# Multiconnect

---

## This project is in its infancy and does not function yet

Sync Android and Linux (maybe Windows?) devices

## How to build/run

First clone the repo then cd into it.

### Desktop

You can specify the config directory with `MC_CONFIG_DIR=<dir>`. This works on both the client and the daemon.
<br />
<br />
**Daemon**
<br />

```
Usage: multiconnectd [-p <port>] [--log-level <log-level>]

Sync devices

Options:
  -p, --port        specify the port of the daemon to run on (default 10999)
  --log-level       specify the log level (default is info)
                    {trace|debug|info|warn|error}
  -h, --help        display usage information
```

```bash
$ cd multiconnect-daemon
$ cargo run dev
```

**Client**
<br />

```
Usage: multiconnect [-p <port>] [--log-level <log-level>]

Sync devices

Options:
  -p, --port        specify the port of the daemon to connect to (default 10999)
  --log-level       specify the log level (default is info)
                    {trace|debug|info|warn|error}
  -h, --help        display usage information
```

```bash
$ cargo tauri dev
```
