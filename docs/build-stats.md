# Build Stats

`build-stats` evaluates an item alone or in a **buildset**: one build per equipment slot.
The calculator is experimental. Reviewed normal-fire profiles cover Vasto, Vasto Prime,
Akvasto Prime, Braton, Braton Prime, Karak, Hek, Boar and Boar Prime. Rules include common ranged
mods, Primary Crux, Mesa's pistol passives, Topaz shards, selected Vasto/Boar evolutions and Ceramic
Dagger / Secondary Outburst interactions, plus Helminth Nourish's weapon buff.
Frame profiles cover Mesa, Mesa Prime, Nova, Nova Prime and Excalibur. Accepting a
build does not imply every mechanic is supported.

```bash
wfcli build-stats 'Vasto Prime' --config 0
wfcli build-stats 'Vasto Prime' --with 'warframe=Mesa Prime' \
  --with 'melee=Ceramic Dagger' --config-for melee=0 --bounds --armor 2700
wfcli build-stats 'Vasto Prime' --effect item/galvanized-diffusion=4
wfcli build-stats 'Vasto Prime' --unmodded --format json
wfcli build-stats Hek --config 0 --bounds --armor 2700
wfcli build-stats 'Vasto Prime' --with 'warframe=Mesa Prime' --effect warframe/nourish=1
wfcli build-stats 'Mesa Prime' --with melee='Ceramic Dagger' --config-for melee=0
wfcli build-stats Nova --unmodded
```

Names match exactly, ignoring case. Use instance IDs for duplicate owned items. Configuration
indices start at `0` (Config A); omitted indices use the cached active configuration. `--with`
adds a contextual slot, `--config-for SLOT=INDEX` selects its configuration. Only selected slots
contribute. `--unmodded` uses a catalog item without requiring ownership; frames use rank 30.
The default target slot is `item`; `--target` names a different slot. Buildsets describe
one player's equipment, not multiple squad members.

## Warframes

Frame targets return health, shields, armor, energy, sprint speed, overshield capacity
and ability multipliers instead of DPS. Rank additions precede mod multipliers.
Overshield capacity does not imply any overshields have been generated. Mesa's flat
health passive applies when the buildset has no melee; include one when equipped.

Reviewed abilities have typed, per-state outputs under `abilities`. Molecular Prime
reports both slow/speed variants, durations, wave radius, explosion damage/radius and
damage vulnerability. Its wave radius scales with duration, not range. These are PvE
cast previews, not damage applied to an enemy. Other abilities remain explicit gaps.
Ability efficiency is a stat multiplier, not a calculated cast or channel cost.

## States

Output lists available effect IDs and caps. `--effect ID=COUNT` holds a condition at the given
count; earned buffs otherwise default to zero. Conditions requiring missing equipment stay
disabled. `--bounds` adds three snapshots:

- **Inactive:** permanent equipped bonuses, no earned buffs.
- **Lower:** Outburst active from the recovered initial combo floor; other earned buffs inactive.
- **Upper:** enabled conditions at their caps, melee at maximum combo.

`--effect melee/combo=COUNT` sets combo at buff activation, including zero before recovery.
Omitting it uses the selected melee's recovered initial combo.

Bounds cover reviewed effects, not a simulated mission. Buildup, activation sequences, buff
expiry and uptime are not modeled. Full ammo saving removes reload losses while active;
magazine damage is then unbounded, but DPS remains finite. Normal ammunition and Incarnon
charges are distinct; transformed Incarnon attacks are not yet evaluated.

Nourish is available when the selected frame configuration has that Helminth override.
It uses the frame's calculated ability strength and duration. Rank-30 equipment supplies
max ability rank; otherwise provide an explicit rank as described below. The buff starts
inactive and is included at its maximum in the upper snapshot. Viral status effects,
retaliation, healing and energy gain are not simulated.

## Buildset Files

Use `--buildset FILE --target SLOT` for saved inputs. Each slot contains either an owned item
selector or an inline normalized build independent of ownership:

```json
{
  "secondary": {"item": "Vasto Prime", "config": 0},
  "warframe": {"item": "Mesa Prime"},
  "melee": {"item": "Ceramic Dagger", "config": 0}
}
```

An inline member uses `{"build": {...}}` with `definition_id`, `class`, and
`config.upgrade_slots`. Upgrades use normalized `topology_slot` IDs, `item_type` and `rank`;
optional `topology`, `skill_tree` and `shard_slots` use the daemon's normalized equipment format.
Do not pass raw game or Overframe slot indices. The native request uses
`{"op":"build_stats","options":{"target":"secondary","buildset":{...},"bounds":true}}`.

For inline frame builds, `config.ability_override` uses `{"Ability":"/Lotus/...","Index":0}`
and `config.ability_ranks` maps ability paths to ranks `0` through `3`. A missing rank is
reported as unsupported unless the equipment XP establishes max rank.
An inline frame's `rank` (`0` through `30`) sets its stat rank; rank 30 also establishes
max ability rank. Owned frames below max XP require explicit rank inputs because
Forma and mastery-dependent ability floors are not inferred.

## Interpretation

Weapon results include expected damage per trigger and magazine, burst DPS and ideal reload-adjusted
DPS, plus per-projectile critical tiers, their probabilities and health damage.
Health damage is rounded for each outcome before averaging. Raw packet damage
precedes target mitigation and final health rounding; nominal elemental damage
precedes packet quantization. These values need not agree even at zero armor.

`--armor` is resolved integer armor for a neutral target, not an enemy or level.
Armor applies per channel: damage at or below 1 is unchanged, larger damage has a floor of 1.
All projectiles hit the body
without falloff. Status damage over time, shields, overguard, faction vulnerabilities and
attenuation remain outside this estimate.
Status chance is per projectile. Base-status additions apply before percentage bonuses;
extra multishot does not reduce it.
Multi-projectile totals sum separate packet expectations and remain partial
until native grouping is verified; they are not a whole-shot outcome distribution.

Unsupported mechanics appear under **Not included** and mark results **partial**. Missing base
stats or unreviewed attack profiles produce errors. JSON includes the contribution graph,
dependency edges, endpoint effects, input fingerprints and player snapshot age. Live damage
parity is not yet verified.

Catalog data follows the [source update policy](data-sources.md); reviewed rules ship with the app.
JSON provenance identifies the mechanic IDs and property roots used. Their shared
dependencies are maintained in the [game-update tracker](wfinspect.md#track-game-updates).
An observed executable differing from the researched build marks the result partial.
