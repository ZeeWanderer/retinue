# Shared Assets

Framework-neutral assets used by multiple applications live here. Qt resources
and Rust embeds consume the same source files. Game-specific assets stay with
their game namespace; downloaded content belongs to the daemon's asset cache.

`warframe/ui/` contains status glyphs shared by the Warframe GUI and companion.

`fonts/roboto/Roboto-{Light,Regular,Medium,Bold}.ttf` are Roboto v2.137, matching
AlecaFrame's embedded family. Source: [Google Fonts archive revision](https://github.com/google/fonts/tree/724bf98e9f5cb98a1d3d5044f45a2e286b817401/apache/roboto).
`fonts/roboto/Roboto-LICENSE.txt` contains its Apache 2.0 license.
