# polyresolver is a resolver for split-horizon scenarios

polyresolver is used to root domain names to different nameservers for the purposes of resolving domains down different pathways. In DNS terms this is called "split horizon DNS" but is usually done at the server-side. This is intended to do it at the client-side. It exposes a server on an IP, `127.0.0.1` by default (but you can override this) and provides a UDP and TCP resolver over port 53. It then scans a configuration directory for YAML files that have this format:

```yaml
domain_name: foo
forwarders:
  - 1.2.3.4
  - 127.0.0.1
  - 192.168.1.1
protocol: udp
```

Config files can be added and removed at any time without restarting the daemon. This makes it ideal for use with e.g., dhcp post-renew scripts.

To launch: `polyresolver <config dir> <(optional) listen ip>`. It runs in the foreground so be sure to supervise it with something.

Does this look familiar? It should, OS X and Windows and systemd (but not Linux!) all have this functionality. polyresolver is most analogous to `systemd-resolved`, just without the dependency on `systemd-networkd` or `systemd` in general. It also should be a cross-platform product also working on Windows, OS X, FreeBSD (anywhere rust and openssl compile, really), allowing this functionality to be used universally in the same way.

## Author

Erik Hollensbe <git@hollensbe.org>

This product makes heavy use (and would not be possible without) the [trust-dns](https://github.com/bluejekyll/trust-dns) rust toolkit.

## License

BSD 3-Clause
