%%%-------------------------------------------------------------------
%% Shared worldstate result-column contract.
%%%-------------------------------------------------------------------
-module(wf_worldstate_schema).

-export([columns_spec/0, column_spec/1, columns_for_type/1,
         default_table_columns/0, columns_for_inventory/1, type_from_label/1,
         query_column_spec/1]).

-type type_atom() :: atom().
-type column() :: atom() | {extra, string()}.
-type column_spec() :: map().

column_spec({extra, Key}) ->
    #{label => Key, role => extra, optional => true};
column_spec(Column) ->
    case lists:dropwhile(fun(Spec) -> maps:get(key, Spec, undefined) =/= Column end, columns_spec()) of
        [] -> #{key => Column, label => atom_to_list(Column)};
        [Spec | _] -> Spec
    end.

-doc "Fallback table columns for worldstate types without a specific column list.".
-spec default_table_columns() -> [column()].
default_table_columns() ->
    [type, summary, location, window, details].

-doc "Columns for inventory subviews, separate from parent Baro/Prime Vault event rows.".
-spec columns_for_inventory(type_atom()) -> [column()].
columns_for_inventory(prime_vault) -> [type, name, price, evergreen];
columns_for_inventory(baro) -> [type, name, ducats, credits, mod];
columns_for_inventory(teshin) -> [name, steel_essence, availability, window];
columns_for_inventory(_) -> default_table_columns().

-doc "Return preferred table columns for a normalized worldstate type.".
-spec columns_for_type(type_atom()) -> [column()].
columns_for_type(alert) -> [type, mission, node, level, faction, reward, expiry];
columns_for_type(fissure) -> [type, tier, mission, node, expiry, hard];
columns_for_type(sortie) -> [type, boss, faction, expiry, stages, modifiers];
columns_for_type(invasion) -> [type, node, sides, progress, attacker_reward, defender_reward];
columns_for_type(void_storm) -> [type, tier, node, window];
columns_for_type(event) -> [type, name, window, link, flags];
columns_for_type(calendar) -> [type, name, window, details];
columns_for_type(global_upgrade) -> [type, name, window, details];
columns_for_type(syndicate_mission) -> [type, name, node, window];
columns_for_type(daily_deal) -> [type, name, pricing, stock, window];
columns_for_type(prime_vault) -> [type, name, node, window, items, featured];
columns_for_type(baro) -> [type, character, node, window];
columns_for_type(prime_vault_item) -> [type, name, price, evergreen];
columns_for_type(void_trader_item) -> [type, name, ducats, credits, mod];
columns_for_type(teshin_item) -> [name, steel_essence, availability, window];
columns_for_type(arbitration) -> [type, node];
columns_for_type(archimedea) -> [type, missions, window, seed];
columns_for_type(circuit) -> [name, choices, window];
columns_for_type(raw_worldstate) -> [type, summary, details];
columns_for_type(meta) -> [meta_key, meta_value];
columns_for_type(_) -> default_table_columns().

-doc "Parse CLI/watch labels back into normalized type atoms.".
-spec type_from_label(term()) -> type_atom() | undefined.
type_from_label(Label0) ->
    Label = string:lowercase(string:trim(retinue_text:to_list(Label0))),
    case Label of
        "alert" -> alert;
        "fissure" -> fissure;
        "sortie" -> sortie;
        "invasion" -> invasion;
        "event" -> event;
        "global" -> global_upgrade;
        "syndicate" -> syndicate_mission;
        "daily deal" -> daily_deal;
        "prime vault" -> prime_vault;
        "baro" -> baro;
        "void storm" -> void_storm;
        "arbitration" -> arbitration;
        "calendar" -> calendar;
        "flash sale" -> flash_sale;
        "goal" -> goal;
        "archimedea" -> archimedea;
        "conquest" -> archimedea;
        "raw worldstate" -> raw_worldstate;
        "construction" -> construction_project;
        "descent" -> descent;
        "circuit" -> circuit;
        "endless xp" -> circuit;
        "experiment" -> experiment_recommended;
        "featured guild" -> featured_guild;
        "hub event" -> hub_event;
        "market" -> in_game_market;
        "library" -> library_info;
        "node override" -> node_override;
        "pvp tournament" -> pvp_active_tournament;
        "pvp mode" -> pvp_alternative_mode;
        "pvp challenge" -> pvp_challenge;
        "persistent enemy" -> persistent_enemy;
        "project pct" -> project_pct;
        "prime access" -> prime_access;
        "prime token" -> prime_token;
        "prime vault availability" -> prime_vault_availability;
        "season info" -> season_info;
        "sku sale" -> sku_sale;
        "twitch promo" -> twitch_promo;
        "lite sortie" -> lite_sortie;
        "meta" -> meta;
        "baro item" -> void_trader_item;
        "prime vault item" -> prime_vault_item;
        "teshin offering" -> teshin_item;
        _ -> undefined
    end.

-doc "Resolve a semantic query alias through the shared projected-column registry.".
-spec query_column_spec(term()) -> {ok, column_spec()} | error.
query_column_spec(Key0) ->
    Key = normalize_query_name(Key0),
    case lists:dropwhile(
           fun(Spec) -> not lists:member(Key, query_names(Spec)) end,
           columns_spec()) of
        [] -> error;
        [Spec | _] -> {ok, Spec}
    end.

query_names(Spec) ->
    Key = maps:get(key, Spec),
    Label = maps:get(label, Spec, ""),
    [normalize_query_name(Key), normalize_query_name(Label) |
     [normalize_query_name(Alias) || Alias <- maps:get(aliases, Spec, [])]].

normalize_query_name(Value) ->
    Lower = string:lowercase(string:trim(retinue_text:to_list(Value))),
    string:replace(string:replace(Lower, "_", "-", all), " ", "-", all).

-doc "Shared column registry; add keys here when new row-map fields need labels/roles.".
-spec columns_spec() -> [column_spec()].
columns_spec() ->
    [
        #{key => type, label => "Type", role => type, optional => true},
        #{key => summary, label => "Summary", role => name},
        #{key => location, label => "Location", role => location},
        #{key => window, label => "Window", role => time, kind => time_range, source => {row_map, [window_start, window_end]}},
        #{key => details, label => "Details", role => details, optional => true},
        #{key => id, label => "ID", role => id, optional => true},
        #{key => name, label => "Name", role => name},
        #{key => choices, label => "Choices", role => details},
        #{key => node, label => "Node", role => location},
        #{key => mission, label => "Mission", role => mission},
        #{key => level, label => "Level", role => stat},
        #{key => tier, label => "Tier", role => stat},
        #{key => expiry, label => "Expiry", role => time, kind => time_point, default_op => eq},
        #{key => window_start, label => "Start", role => time, kind => time_point, default_op => eq, optional => true},
        #{key => window_end, label => "End", role => time, kind => time_point, default_op => eq, optional => true},
        #{key => boss, label => "Boss", role => name},
        #{key => faction, label => "Faction", role => stat},
        #{key => reward, label => "Reward", role => details},
        #{key => sides, label => "Sides", role => details, optional => true},
        #{key => progress, label => "Progress", role => stat},
        #{key => stages, label => "Stages", role => details, optional => true},
        #{key => modifiers, label => "Modifiers", role => details, optional => true},
        #{key => link, label => "Link", role => link, optional => true},
        #{key => flags, label => "Flags", role => flags, optional => true},
        #{key => attacker, label => "Attacker", role => name},
        #{key => defender, label => "Defender", role => name},
        #{key => attacker_reward, label => "Attacker reward", role => details, optional => true},
        #{key => defender_reward, label => "Defender reward", role => details, optional => true},
        #{key => pricing, label => "Pricing", role => details, optional => true},
        #{key => stock, label => "Stock", role => stat, optional => true},
        #{key => items, label => "Items", role => details, optional => true},
        #{key => featured, label => "Featured", role => details, optional => true},
        #{key => price, label => "Price", role => stat},
        #{key => ducats, label => "Ducats", role => stat},
        #{key => credits, label => "Credits", role => stat},
        #{key => steel_essence, label => "Steel Essence", role => stat},
        #{key => availability, label => "Availability", role => stat},
        #{key => mod, label => "Mod", role => details, optional => true},
        #{key => evergreen, label => "Evergreen", role => stat, optional => true},
        #{key => character, label => "Character", role => name},
        #{key => hard, label => "Hard", role => stat, optional => true},
        #{key => archimedea, label => "Archimedea", role => name,
          aliases => ["archimedea-kind"], default_op => eq},
        #{key => missions, label => "Missions", role => details,
          aliases => ["mission"]},
        #{key => deviations, label => "Deviations", role => details,
          aliases => ["deviation"]},
        #{key => risks, label => "Risks", role => details,
          aliases => ["risk"]},
        #{key => elite_risks, label => "Elite risks", role => details,
          aliases => ["elite-risk", "elite-risks"]},
        #{key => personal_modifiers, label => "Personal modifiers", role => details,
          aliases => ["personal-modifier", "modifier"]},
        #{key => mission_details, label => "Mission details", role => details,
          aliases => ["mission-detail"]},
        #{key => modifier_details, label => "Modifier details", role => details,
          aliases => ["modifier-detail"]},
        #{key => seed, label => "Seed", role => stat, kind => number, default_op => eq},
        #{key => loadouts, label => "Loadouts", role => details, optional => true},
        #{key => loadout_status, label => "Loadout status", role => stat,
          aliases => ["loadout-status"], default_op => eq, optional => true},
        #{key => loadout_seed, label => "Loadout seed", role => stat,
          aliases => ["loadout-seed"], kind => number, default_op => eq, optional => true},
        #{key => suits, label => "Warframes", role => details,
          aliases => ["warframe", "warframes"], optional => true},
        #{key => primaries, label => "Primary", role => details,
          aliases => ["primary"], optional => true},
        #{key => secondaries, label => "Secondary", role => details,
          aliases => ["secondary"], optional => true},
        #{key => melees, label => "Melee", role => details,
          aliases => ["melee"], optional => true},
        #{key => meta_key, label => "Key", role => name},
        #{key => meta_value, label => "Value", role => details}
    ].
