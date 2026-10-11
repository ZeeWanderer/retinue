# ARC Raiders

`arccompanion` is a launch scaffold. Gameplay observation and overlays are not implemented.
It does not connect to the daemon or create persistent data.

```bash
retinue arc companion status
arccompanion status --json
```

To run it through Steam, use the installed binary's absolute path:

```text
/absolute/path/to/arccompanion launch -- %command%
```

Launch replaces the scaffold with Steam's command, preserving arguments, signals and exit status.
`arccli` is the alias for `retinue arc`.
