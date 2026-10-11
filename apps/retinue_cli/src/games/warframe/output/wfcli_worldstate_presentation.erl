%%%-------------------------------------------------------------------
%% Terminal block layouts for worldstate entities.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_presentation).

-export([block_spec/1]).

-type type_atom() :: atom().

-doc "Return block-view title and fields; add new worldstate types here for nicer detail output.".
-spec block_spec(type_atom()) -> map().
block_spec(Type) ->
    case Type of
        alert ->
            #{title => "Alert",
              fields => [{"Node", node}, {"Mission", mission}, {"Level", level},
                         {"Faction", faction}, {"Reward", reward}, {"Expires", expiry}]};
        fissure ->
            #{title => "Fissure",
              fields => [{"Tier", tier}, {"Mission", mission}, {"Node", node},
                         {"Hard", hard}, {"Expires", expiry}]};
        sortie ->
            #{title => "Sortie",
              fields => [{"Boss", boss}, {"Faction", faction}, {"Stages", stages},
                         {"Modifiers", modifiers}, {"Expires", expiry}]};
        invasion ->
            #{title => "Invasion",
              fields => [{"Node", node}, {"Attacker", attacker}, {"Defender", defender},
                         {"Progress", progress}, {"Attacker reward", attacker_reward},
                         {"Defender reward", defender_reward}]};
        event ->
            #{title => "Event: {summary}",
              fields => [{"Window", window}, {"Link", link}, {"Flags", flags}, {"Details", details}]};
        global_upgrade ->
            #{title => "Global upgrade: {summary}",
              fields => [{"Window", window}, {"Effect", details}]};
        syndicate_mission ->
            #{title => "Syndicate mission: {summary}",
              fields => [{"Window", window}, {"Nodes", node}]};
        daily_deal ->
            #{title => "Daily deal: {summary}",
              fields => [{"Window", window}, {"Pricing", pricing}, {"Stock", stock}]};
        prime_vault ->
            #{title => "Prime vault",
              fields => [{"Node", node}, {"Window", window}, {"Items", items}, {"Featured", featured}]};
        void_trader_item ->
            #{title => "Baro item: {summary}",
              fields => [{"Price", price}, {"Mod", mod}]};
        prime_vault_item ->
            #{title => "Prime vault item: {summary}",
              fields => [{"Price", price}, {"Evergreen", evergreen}]};
        teshin_item ->
            #{title => "Teshin offering: {summary}",
              fields => [{"Steel Essence", steel_essence},
                         {"Availability", availability}, {"Window", window}],
              skip_fields => [window_start, window_end, activation, expiry,
                              cost, details, id, name, summary, type]};
        baro ->
            #{title => "Baro: {summary}",
              fields => [{"Node", node}, {"Window", window}, {"Character", character}]};
        void_storm ->
            #{title => "Void storm",
              fields => [{"Tier", tier}, {"Node", node}, {"Window", window}]};
        arbitration ->
            #{title => "Arbitration",
              fields => [{"Node", node}]};
        calendar ->
            #{title => "{summary}",
              fields => [{"Window", window}, {"Events", details}]};
        flash_sale ->
            #{title => "Flash sale: {summary}",
              fields => [{"Window", window}, {"Visibility", details}]};
        goal ->
            #{title => "Goal: {summary}",
              fields => [{"Node", node}, {"Window", window}, {"Progress", progress}, {"Reward", reward}]};
        archimedea ->
            #{title => "{summary}",
              fields => [{"Window", window}, {"Missions", mission_details},
                         {"Personal modifiers", modifier_details}, {"Random seed", seed},
                         {"Warning", loadout_warning}, {"Loadouts", loadouts}],
              skip_fields => [window_start, window_end, id, name, type, summary,
                              archimedea, missions, deviations, risks, elite_risks,
                              personal_modifiers, randomseed, loadout_status, loadout_seed,
                              suits, primaries, secondaries, melees, details]};
        construction_project ->
            #{title => "Construction project",
              fields => [{"Node", node}, {"Window", window}, {"Progress", progress}]};
        descent ->
            #{title => "Descent",
              fields => [{"Window", window}, {"Challenges", details}]};
        circuit ->
            #{title => "Circuit: {summary}",
              fields => [{"Choices", details}, {"Week", window}],
              skip_fields => [name, category, choices, window_start, window_end]};
        experiment_recommended ->
            #{title => "Experiment recommended",
              fields => [{"Tag", summary}, {"Score", details}]};
        featured_guild ->
            #{title => "Featured guild",
              fields => [{"Name", summary}, {"Tier", tier}, {"Alliance", alliance}]};
        hub_event ->
            #{title => "Hub event",
              fields => [{"Tag", summary}, {"Node", node}, {"Window", window}]};
        in_game_market ->
            #{title => "{summary}",
              fields => [{"Categories", categories}]};
        library_info ->
            #{title => "Library info",
              fields => [{"Last target", last_target}]};
        node_override ->
            #{title => "Node override",
              fields => [{"Node", node}, {"Hide", hide}]};
        pvp_active_tournament ->
            #{title => "PVP tournament",
              fields => [{"Mode", mode}, {"Window", window}]};
        pvp_alternative_mode ->
            #{title => "PVP alternative mode",
              fields => [{"Name", summary}, {"Mode", mode}]};
        pvp_challenge ->
            #{title => "PVP challenge",
              fields => [{"Category", summary}, {"Window", window}, {"Subchallenges", subchallenges}]};
        persistent_enemy ->
            #{title => "Persistent enemy",
              fields => [{"Name", summary}, {"Rank", rank}]};
        project_pct ->
            #{title => "Project percentage",
              fields => [{"Index", index}, {"Percent", percent}]};
        prime_access ->
            #{title => "Prime access",
              fields => [{"State", state}]};
        prime_token ->
            #{title => "Prime token availability",
              fields => [{"Available", available}]};
        prime_vault_availability ->
            #{title => "Prime vault availability",
              fields => [{"Index", index}, {"Available", available}]};
        season_info ->
            #{title => "Season info: {summary}",
              fields => [{"Window", window}, {"Phase", phase}, {"Challenges", challenges}]};
        sku_sale ->
            #{title => "SKU sale",
              fields => [{"Type", sale_type}, {"Window", window}]};
        twitch_promo ->
            #{title => "Twitch promo",
              fields => [{"Name", summary}, {"Window", window}]};
        lite_sortie ->
            #{title => "Lite sortie",
              fields => [{"Boss", summary}, {"Window", window}, {"Missions", missions}, {"Reward", reward}]};
        meta ->
            #{title => "Worldstate meta",
              fields => [{"Key", meta_key}, {"Value", meta_value}]};
        _ ->
            #{title => "Entry", fields => []}
    end.
