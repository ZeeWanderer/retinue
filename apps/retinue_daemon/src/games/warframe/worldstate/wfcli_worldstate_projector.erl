%%%-------------------------------------------------------------------
%% Semantic display-field projection for worldstate entries.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_projector).

-export([table_row_map/2, expiry/1, expiry/2, to_list/1, circuit_category/1,
         event_name/2, global_upgrade_name/2, syndicate_name/2, daily_deal_name/2, prime_vault_name/2,
         goal_name/2, season_info_name/2, in_game_market_name/2, library_info_name/2,
         resolve_extra_field/3]).

table_row_map(#{type := fissure, data := D} = Entry, Opts) ->
    Tier = wfcli_resolve:resolve("modifier", maps:get(<<"Modifier">>, D, <<"Unknown">>), Opts),
    Mission = wfcli_resolve:resolve("missiontype", maps:get(<<"MissionType">>, D, <<"">>), Opts),
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    Exp = expiry(maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Fissure",
        summary => lists:flatten(io_lib:format("~s ~s", [Tier, Mission])),
        location => Node,
        node => Node,
        window => Exp,
        expiry => Exp,
        tier => Tier,
        mission => Mission,
        hard => format_hard(maps:get(<<"Hard">>, D, undefined))
    });
table_row_map(#{type := sortie, data := D, name := Name} = Entry, Opts) ->
    Boss = sortie_boss_name(maps:get(<<"Boss">>, D, Name), Opts),
    Faction = wfcli_resolve:resolve("faction", maps:get(<<"Faction">>, D, <<"Unknown">>), Opts),
    Exp = expiry(maps:get(<<"Expiry">>, D, undefined), Opts),
    {Stages, Modifiers} = sortie_variants(D, Opts),
    with_id(Entry, #{
        type => "Sortie",
        summary => Boss,
        window => Exp,
        expiry => Exp,
        boss => Boss,
        faction => Faction,
        stages => Stages,
        modifiers => Modifiers,
        details => lists:flatten(io_lib:format("Faction: ~s", [Faction]))
    });
table_row_map(#{type := alert, data := D, name := Name} = Entry, Opts) ->
    Exp = expiry(maps:get(<<"Expiry">>, D, undefined), Opts),
    Mission = alert_mission_type(D, Opts),
    Node = alert_node(D, Name, Opts),
    Level = alert_level_range(D),
    Faction = alert_faction(D, Opts),
    Reward = alert_reward(D, Opts),
    with_id(Entry, #{
        type => "Alert",
        summary => Mission,
        location => Node,
        node => Node,
        window => Exp,
        expiry => Exp,
        mission => Mission,
        level => Level,
        faction => Faction,
        reward => Reward
    });
table_row_map(#{type := invasion, data := D, name := Name} = Entry, Opts) ->
    Att = wfcli_resolve:resolve("faction", get_faction(D, attacker), Opts),
    Def = wfcli_resolve:resolve("faction", get_faction(D, defender), Opts),
    {AttRw0, DefRw0} = invasion_rewards(D, Opts),
    AttRw = case AttRw0 of "none" -> ""; _ -> AttRw0 end,
    DefRw = case DefRw0 of "none" -> ""; _ -> DefRw0 end,
    Node = wfcli_resolve:resolve("node", Name, Opts),
    Progress = invasion_progress(D),
    with_id(Entry, #{
        type => "Invasion",
        summary => lists:flatten(io_lib:format("~s vs ~s", [Att, Def])),
        location => Node,
        node => Node,
        attacker => Att,
        defender => Def,
        sides => lists:flatten(io_lib:format("~s vs ~s", [Att, Def])),
        progress => Progress,
        attacker_reward => AttRw,
        defender_reward => DefRw,
        details => lists:flatten(io_lib:format("Attacker: ~s; Defender: ~s", [AttRw, DefRw]))
    });
table_row_map(#{type := event, data := D} = Entry, Opts) ->
    Msg = event_name(D, Opts),
    {Window, Start, End} = event_window_parts(D, Opts),
    Prop = event_prop(D),
    with_id(Entry, #{
        type => "Event",
        summary => Msg,
        window => Window,
        window_start => Start,
        window_end => End,
        details => Prop,
        name => Msg,
        link => event_link(D),
        flags => event_flags(D)
    });
table_row_map(#{type := global_upgrade, data := D} = Entry, Opts) ->
    Name = global_upgrade_name(D, Opts),
    {Window, Start, End} = global_upgrade_window_parts(D, Opts),
    Upgrade = global_upgrade_type(D, Opts),
    with_id(Entry, #{
        type => "Global",
        summary => Name,
        window => Window,
        window_start => Start,
        window_end => End,
        details => Upgrade,
        name => Name
    });
table_row_map(#{type := syndicate_mission, data := D} = Entry, Opts) ->
    Name = syndicate_name(D, Opts),
    {Window, Start, End} = syndicate_window_parts(D, Opts),
    Nodes = syndicate_nodes(D, Opts),
    with_id(Entry, #{
        type => "Syndicate",
        summary => Name,
        location => Nodes,
        node => Nodes,
        window => Window,
        window_start => Start,
        window_end => End,
        name => Name
    });
table_row_map(#{type := daily_deal, data := D} = Entry, Opts) ->
    Name = daily_deal_name(D, Opts),
    {Window, Start, End} = daily_deal_window_parts(D, Opts),
    Pricing = daily_deal_pricing(D),
    Stock = daily_deal_stock(D),
    Details = join_parts([Pricing, Stock], "; "),
    with_id(Entry, #{
        type => "Daily Deal",
        summary => Name,
        window => Window,
        window_start => Start,
        window_end => End,
        pricing => Pricing,
        stock => Stock,
        details => Details,
        name => Name
    });
table_row_map(#{type := prime_vault, data := D} = Entry, Opts) ->
    Name = prime_vault_name(D, Opts),
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    {Window, Start, End} = prime_vault_window_parts(D, Opts),
    Items = prime_vault_items(D, Opts),
    Featured = prime_vault_featured(D, Opts),
    with_id(Entry, #{
        type => "Prime Vault",
        summary => Name,
        location => Node,
        node => Node,
        window => Window,
        window_start => Start,
        window_end => End,
        items => Items,
        details => Items,
        featured => Featured,
        name => Name
    });
table_row_map(#{type := void_trader_item, data := D} = Entry, Opts) ->
    Name = wfcli_resolve:resolve("item", maps:get(<<"ItemType">>, D, <<"Unknown">>), Opts),
    Price = void_trader_price(D),
    {Ducats, Credits} = price_parts(D),
    ModInfo = mod_details_suffix(maps:get(<<"ItemType">>, D, undefined), Opts),
    Details = join_parts([Price, ModInfo], " | "),
    with_id(Entry, #{
        type => "Baro Item",
        summary => Name,
        ducats => Ducats,
        credits => Credits,
        price => Price,
        mod => ModInfo,
        details => Details,
        name => Name
    });
table_row_map(#{type := prime_vault_item, data := D} = Entry, Opts) ->
    Name = wfcli_resolve:resolve("item", maps:get(<<"ItemType">>, D, <<"Unknown">>), Opts),
    Price = void_trader_price(D),
    Evergreen = case maps:get(<<"Evergreen">>, D, false) of
        true -> "Evergreen";
        _ -> ""
    end,
    Details = join_parts([Price, Evergreen], " | "),
    with_id(Entry, #{
        type => "Prime Vault Item",
        summary => Name,
        price => Price,
        evergreen => Evergreen,
        details => Details,
        name => Name
    });
table_row_map(#{type := teshin_item, data := D, name := Name} = Entry, Opts) ->
    Cost = maps:get(<<"Cost">>, D, ""),
    Availability = to_list(maps:get(<<"Availability">>, D, <<"">>)),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Teshin Offering",
        summary => to_list(Name),
        name => to_list(Name),
        steel_essence => Cost,
        cost => Cost,
        availability => Availability,
        window => Window,
        window_start => Start,
        window_end => End,
        activation => Start,
        expiry => End,
        details => Availability
    });
table_row_map(#{type := baro, data := D, name := Name} = Entry, Opts) ->
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Baro",
        summary => to_list(Name),
        location => Node,
        node => Node,
        window => Window,
        window_start => Start,
        window_end => End,
        name => to_list(Name),
        character => to_list(Name)
    });
table_row_map(#{type := void_storm, data := D} = Entry, Opts) ->
    Tier = wfcli_resolve:resolve("modifier", maps:get(<<"ActiveMissionTier">>, D, <<"Unknown">>), Opts),
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Void Storm",
        summary => Tier,
        location => Node,
        node => Node,
        window => Window,
        window_start => Start,
        window_end => End,
        tier => Tier
    });
table_row_map(#{type := arbitration, name := Name} = Entry, Opts) ->
    Node = wfcli_resolve:resolve("node", Name, Opts),
    with_id(Entry, #{
        type => "Arbitration",
        location => Node,
        node => Node
    });
table_row_map(#{type := calendar, data := D} = Entry, Opts) ->
    Title = calendar_title(D, Opts),
    {Window, Start, End} = calendar_window_parts(D, Opts),
    Events = calendar_events(D, Opts),
    with_id(Entry, #{
        type => "Calendar",
        name => Title,
        summary => Title,
        window => Window,
        window_start => Start,
        window_end => End,
        details => Events
    });
table_row_map(#{type := flash_sale, data := D} = Entry, Opts) ->
    Name = wfcli_resolve:resolve("item", maps:get(<<"TypeName">>, D, <<"Unknown">>), Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"StartDate">>, D, undefined),
                                                  maps:get(<<"EndDate">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Flash sale",
        summary => Name,
        window => Window,
        window_start => Start,
        window_end => End,
        details => flash_sale_visibility(D),
        name => Name
    });
table_row_map(#{type := goal, data := D} = Entry, Opts) ->
    Name = goal_name(D, Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    Progress = goal_progress(D),
    Reward = fmt_reward(maps:get(<<"Reward">>, D, #{}), Opts),
    with_id(Entry, #{
        type => "Goal",
        summary => Name,
        location => Node,
        node => Node,
        window => Window,
        window_start => Start,
        window_end => End,
        progress => Progress,
        reward => Reward,
        details => join_parts([Progress, Reward], " | "),
        name => Name
    });
table_row_map(#{type := archimedea, data := D} = Entry, Opts) ->
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    Projected = wfcli_archimedea:project(D, Opts),
    with_id(Entry, Projected#{
        window => Window,
        window_start => Start,
        window_end => End
    });
table_row_map(#{type := construction_project, data := D} = Entry, Opts) ->
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    Progress = to_list(maps:get(<<"Progress">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Construction",
        summary => Node,
        location => Node,
        node => Node,
        window => Window,
        window_start => Start,
        window_end => End,
        details => Progress
    });
table_row_map(#{type := descent, data := D} = Entry, Opts) ->
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    Challenges = descent_challenges(D, Opts),
    with_id(Entry, #{
        type => "Descent",
        summary => "Descent",
        window => Window,
        window_start => Start,
        window_end => End,
        details => Challenges
    });
table_row_map(#{type := circuit, data := D} = Entry, Opts) ->
    Category = circuit_category(maps:get(<<"Category">>, D)),
    Choices = join_list(maps:get(<<"Choices">>, D, []), ", "),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                 maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Circuit",
        summary => Category,
        category => Category,
        choices => Choices,
        details => Choices,
        window => Window,
        window_start => Start,
        window_end => End,
        name => Category
    });
table_row_map(#{type := experiment_recommended, data := D} = Entry, _Opts) ->
    Tag = to_list(maps:get(<<"Tag">>, D, <<"Experiment">>)),
    Score = to_list(maps:get(<<"Score">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Experiment",
        summary => Tag,
        tag => Tag,
        score => Score,
        details => Score,
        name => Tag
    });
table_row_map(#{type := featured_guild, data := D} = Entry, _Opts) ->
    Name = to_list(maps:get(<<"Name">>, D, <<"Guild">>)),
    Tier = to_list(maps:get(<<"Tier">>, D, <<"">>)),
    Alliance = oid_string(maps:get(<<"AllianceId">>, D, undefined)),
    with_id(Entry, #{
        type => "Featured Guild",
        summary => Name,
        tier => Tier,
        alliance => Alliance,
        details => join_parts([Tier, Alliance], " | "),
        name => Name
    });
table_row_map(#{type := hub_event, data := D} = Entry, Opts) ->
    Tag = to_list(maps:get(<<"Tag">>, D, <<"Hub Event">>)),
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Hub Event",
        summary => Tag,
        location => Node,
        node => Node,
        window => Window,
        window_start => Start,
        window_end => End,
        tag => Tag,
        name => Tag
    });
table_row_map(#{type := in_game_market, data := D} = Entry, Opts) ->
    Title = in_game_market_name(D, Opts),
    Categories = in_game_market_categories(D, Opts),
    with_id(Entry, #{
        type => "Market",
        summary => Title,
        categories => Categories,
        details => Categories,
        name => Title
    });
table_row_map(#{type := library_info, data := D} = Entry, Opts) ->
    Name = library_info_name(D, Opts),
    with_id(Entry, #{
        type => "Library",
        summary => Name,
        last_target => Name,
        name => Name
    });
table_row_map(#{type := node_override, data := D} = Entry, Opts) ->
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, D, <<"Unknown">>), Opts),
    Hide = to_list(maps:get(<<"Hide">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Node Override",
        summary => Node,
        location => Node,
        node => Node,
        hide => Hide,
        details => Hide
    });
table_row_map(#{type := pvp_active_tournament, data := D} = Entry, Opts) ->
    Mode = to_list(maps:get(<<"PVPMode">>, D, <<"PVP Tournament">>)),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"startDate">>, D, undefined),
                                                  maps:get(<<"endDate">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "PVP Tournament",
        summary => Mode,
        mode => Mode,
        window => Window,
        window_start => Start,
        window_end => End,
        name => Mode
    });
table_row_map(#{type := pvp_alternative_mode, data := D} = Entry, _Opts) ->
    Name = to_list(maps:get(<<"Name">>, D, <<"PVP Mode">>)),
    Mode = to_list(maps:get(<<"Mode">>, D, <<"">>)),
    with_id(Entry, #{
        type => "PVP Mode",
        summary => Name,
        mode => Mode,
        details => Mode,
        name => Name
    });
table_row_map(#{type := pvp_challenge, data := D} = Entry, Opts) ->
    Category = to_list(maps:get(<<"Category">>, D, <<"PVP Challenge">>)),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"startDate">>, D, undefined),
                                                  maps:get(<<"endDate">>, D, undefined), Opts),
    Count = to_list(length(maps:get(<<"subChallenges">>, D, []))),
    with_id(Entry, #{
        type => "PVP Challenge",
        summary => Category,
        window => Window,
        window_start => Start,
        window_end => End,
        subchallenges => Count,
        details => Count,
        name => Category
    });
table_row_map(#{type := persistent_enemy, data := D} = Entry, Opts) ->
    Name = wfcli_resolve:resolve("item", maps:get(<<"AgentType">>, D, <<"Unknown">>), Opts),
    Rank = to_list(maps:get(<<"Rank">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Persistent Enemy",
        summary => Name,
        rank => Rank,
        details => Rank,
        name => Name
    });
table_row_map(#{type := project_pct, data := D} = Entry, _Opts) ->
    Index = to_list(maps:get(<<"Index">>, D, <<"">>)),
    Pct = to_list(maps:get(<<"Pct">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Project Pct",
        summary => Index,
        index => Index,
        percent => Pct,
        details => Pct
    });
table_row_map(#{type := prime_access, data := D} = Entry, _Opts) ->
    State = to_list(maps:get(<<"State">>, D, <<"Unknown">>)),
    with_id(Entry, #{
        type => "Prime Access",
        summary => State,
        state => State,
        name => State
    });
table_row_map(#{type := prime_token, data := D} = Entry, _Opts) ->
    Val = to_list(maps:get(<<"PrimeTokenAvailability">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Prime Token",
        summary => Val,
        available => Val,
        details => Val
    });
table_row_map(#{type := prime_vault_availability, data := D} = Entry, _Opts) ->
    Index = to_list(maps:get(<<"Index">>, D, <<"">>)),
    Val = to_list(maps:get(<<"Available">>, D, <<"">>)),
    with_id(Entry, #{
        type => "Prime Vault Availability",
        summary => Index,
        index => Index,
        available => Val,
        details => Val
    });
table_row_map(#{type := season_info, data := D} = Entry, Opts) ->
    Name = season_info_name(D, Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    Phase = to_list(maps:get(<<"Phase">>, D, <<"">>)),
    Count = to_list(length(maps:get(<<"ActiveChallenges">>, D, []))),
    with_id(Entry, #{
        type => "Season Info",
        summary => Name,
        window => Window,
        window_start => Start,
        window_end => End,
        phase => Phase,
        challenges => Count,
        details => Count,
        name => Name
    });
table_row_map(#{type := sku_sale, data := D} = Entry, Opts) ->
    Name = to_list(maps:get(<<"SaleType">>, D, <<"SKU Sale">>)),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"SaleStartDate">>, D, undefined),
                                                  maps:get(<<"SaleEndDate">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "SKU Sale",
        summary => Name,
        sale_type => Name,
        window => Window,
        window_start => Start,
        window_end => End,
        name => Name
    });
table_row_map(#{type := twitch_promo, data := D} = Entry, Opts) ->
    Name = to_list(maps:get(<<"Name">>, D, <<"Twitch Promo">>)),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    with_id(Entry, #{
        type => "Twitch Promo",
        summary => Name,
        window => Window,
        window_start => Start,
        window_end => End,
        name => Name
    });
table_row_map(#{type := lite_sortie, data := D} = Entry, Opts) ->
    Boss = sortie_boss_name(maps:get(<<"Boss">>, D, <<"Lite Sortie">>), Opts),
    {Window, Start, End} = window_from_dates_parts(maps:get(<<"Activation">>, D, undefined),
                                                  maps:get(<<"Expiry">>, D, undefined), Opts),
    Missions = lite_sortie_missions(D, Opts),
    Reward = wfcli_resolve:resolve("item", maps:get(<<"Reward">>, D, <<"">>), Opts),
    with_id(Entry, #{
        type => "Lite Sortie",
        summary => Boss,
        window => Window,
        window_start => Start,
        window_end => End,
        missions => Missions,
        reward => Reward,
        details => Missions,
        name => Boss
    });
table_row_map(#{type := meta, data := D} = Entry, _Opts) ->
    {Key, Val} = meta_kv(D),
    with_id(Entry, #{
        type => "Meta",
        summary => to_list(Key),
        meta_key => to_list(Key),
        meta_value => to_list(Val),
        details => to_list(Val),
        name => to_list(Key)
    });
table_row_map(Other, _Opts) ->
    #{type => "Unknown",
      summary => lists:flatten(io_lib:format("~p", [Other]))}.

join_parts(Parts, Sep) ->
    retinue_text:join_parts(Parts, Sep).

join_list(List, Sep) ->
    retinue_text:join_list(List, Sep).

with_id(Entry, Map) ->
    case maps:get(id, Entry, undefined) of
        undefined -> Map;
        Id -> Map#{id => to_list(Id)}
    end.

flash_sale_visibility(D) ->
    Hide = maps:get(<<"HideFromMarket">>, D, false),
    Show = maps:get(<<"ShowInMarket">>, D, false),
    case {Hide, Show} of
        {true, _} -> "hidden";
        {_, true} -> "visible";
        _ -> ""
    end.

goal_name(D, Opts) ->
    Raw = first_value([<<"Desc">>, <<"ToolTip">>, <<"Tag">>], D),
    case {Raw, maps:get(resolve_items, Opts, false)} of
        {undefined, _} -> "Goal";
        {_, false} -> to_list(Raw);
        {_, true} -> wfcli_resolve:resolve("any", Raw, Opts)
    end.

goal_progress(D) ->
    Count = maps:get(<<"Count">>, D, undefined),
    Goal = maps:get(<<"Goal">>, D, undefined),
    case {Count, Goal} of
        {C, G} when is_integer(C), is_integer(G) ->
            lists:flatten(io_lib:format("~p/~p", [C, G]));
        _ -> ""
    end.

season_info_name(D, Opts) ->
    Season = maps:get(<<"Season">>, D, <<"Unknown">>),
    case maps:get(resolve_items, Opts, false) of
        false -> lists:flatten(io_lib:format("Season ~s", [to_list(Season)]));
        true ->
            Resolved = wfcli_resolve:resolve("season", Season, Opts),
            case Resolved =:= to_list(Season) of
                true -> lists:flatten(io_lib:format("Season ~s", [to_list(Season)]));
                false -> Resolved
            end
    end.

descent_challenges(D, Opts) ->
    Challenges = maps:get(<<"Challenges">>, D, []),
    Types = [
        wfcli_resolve:resolve("dt", maps:get(<<"Type">>, C, <<"">>), Opts)
        || C <- Challenges, is_map(C)
    ],
    Count = length(Challenges),
    Summary = join_list(Types, ", "),
    case {Count, Summary} of
        {0, _} -> "none";
        {_, ""} -> lists:flatten(io_lib:format("~p challenges", [Count]));
        _ -> lists:flatten(io_lib:format("~p challenges: ~s", [Count, Summary]))
    end.

in_game_market_name(D, _Opts) ->
    Cats = in_game_market_categories_list(D),
    lists:flatten(io_lib:format("Market (~p categories)", [length(Cats)])).

in_game_market_categories(D, Opts) ->
    Cats = in_game_market_categories_list(D),
    Names = [market_category_name(C, Opts) || C <- Cats, is_map(C)],
    join_list(Names, " | ").

in_game_market_categories_list(D) ->
    Landing = maps:get(<<"LandingPage">>, D, #{}),
    maps:get(<<"Categories">>, Landing, []).

market_category_name(Cat, Opts) ->
    Raw = maps:get(<<"Name">>, Cat, maps:get(<<"CategoryName">>, Cat, <<"Category">>)),
    case maps:get(resolve_items, Opts, false) of
        false -> to_list(Raw);
        true -> wfcli_resolve:resolve("any", Raw, Opts)
    end.

library_info_name(D, Opts) ->
    wfcli_resolve:resolve("item", maps:get(<<"LastCompletedTargetType">>, D, <<"Unknown">>), Opts).

lite_sortie_missions(D, Opts) ->
    Missions = maps:get(<<"Missions">>, D, []),
    Labels = [lite_sortie_mission_label(M, Opts) || M <- Missions, is_map(M)],
    join_list(Labels, " | ").

lite_sortie_mission_label(M, Opts) ->
    Mission = wfcli_resolve:resolve("missiontype", maps:get(<<"missionType">>, M, maps:get(<<"MissionType">>, M, <<"">>)), Opts),
    Node0 = maps:get(<<"node">>, M, maps:get(<<"Node">>, M, undefined)),
    case value_present(Node0) of
        true -> lists:flatten(io_lib:format("~s @ ~s", [Mission, wfcli_resolve:resolve("node", Node0, Opts)]));
        false -> Mission
    end.

meta_kv(D) ->
    [{Key, Val} | _] = maps:to_list(D),
    {Key, Val}.

oid_string(#{<<"$oid">> := Oid}) -> to_list(Oid);
oid_string(Val) -> to_list(Val).

alert_mission_info(D) ->
    maps:get(<<"MissionInfo">>, D, #{}).

alert_mission_type(D, Opts) ->
    Info = alert_mission_info(D),
    Mission = case maps:get(<<"missionType">>, Info, undefined) of
        undefined -> maps:get(<<"MissionType">>, Info, undefined);
        Val -> Val
    end,
    wfcli_resolve:resolve("missiontype", Mission, Opts).

alert_node(D, Name, Opts) ->
    Info = alert_mission_info(D),
    Loc = maps:get(<<"location">>, Info, undefined),
    Node0 = maps:get(<<"Node">>, D, undefined),
    Node = case value_present(Node0) of
        true -> Node0;
        false -> Loc
    end,
    wfcli_resolve:resolve("node", case value_present(Node) of true -> Node; false -> Name end, Opts).

alert_level_range(D) ->
    Info = alert_mission_info(D),
    Min = maps:get(<<"minEnemyLevel">>, Info, undefined),
    Max = maps:get(<<"maxEnemyLevel">>, Info, undefined),
    case {Min, Max} of
        {undefined, undefined} -> "";
        {V, undefined} -> io_lib:format("~p", [V]);
        {undefined, V} -> io_lib:format("~p", [V]);
        {A, B} -> io_lib:format("~p-~p", [A, B])
    end.

alert_faction(D, Opts) ->
    Info = alert_mission_info(D),
    Val = maps:get(<<"faction">>, Info, undefined),
    case value_present(Val) of
        true -> wfcli_resolve:resolve("faction", Val, Opts);
        false -> ""
    end.

alert_reward(D, Opts) ->
    Info = alert_mission_info(D),
    Reward = maps:get(<<"missionReward">>, Info, #{}),
    case fmt_reward(Reward, Opts) of
        "none" -> "";
        Val -> Val
    end.

invasion_progress(D) ->
    Count = maps:get(<<"Count">>, D, undefined),
    Goal = maps:get(<<"Goal">>, D, undefined),
    Completed = maps:get(<<"Completed">>, D, false),
    case {Count, Goal} of
        {undefined, _} -> "";
        {_, undefined} -> "";
        {C, G} when is_integer(C), is_integer(G) ->
            Side = case C >= 0 of true -> "attacker"; false -> "defender" end,
            Base = lists:flatten(io_lib:format("~s ~p/~p", [Side, abs(C), G])),
            case Completed of
                true -> Base ++ " (complete)";
                _ -> Base
            end;
        _ -> ""
    end.

event_link(Map) ->
    case maps:get(<<"Prop">>, Map, undefined) of
        undefined ->
            Links = maps:get(<<"Links">>, Map, []),
            case Links of
                [First | _] -> maps:get(<<"Link">>, First, "");
                _ -> ""
            end;
        Val -> to_list(Val)
    end.

event_flags(Map) ->
    Flags = [
        flag_label(maps:get(<<"Priority">>, Map, false), "priority"),
        flag_label(maps:get(<<"Community">>, Map, false), "community"),
        flag_label(maps:get(<<"MobileOnly">>, Map, false), "mobile")
    ],
    string:join([F || F <- Flags, F =/= ""], ", ").

flag_label(true, Label) -> Label;
flag_label(_, _) -> "".

prime_vault_featured(Map, Opts) ->
    Schedule = maps:get(<<"ScheduleInfo">>, Map, []),
    Featured = case Schedule of
        [First | _] -> maps:get(<<"FeaturedItem">>, First, undefined);
        _ -> undefined
    end,
    case Featured of
        undefined -> "";
        Val -> wfcli_resolve:resolve("item", Val, Opts)
    end.

price_parts(Map) ->
    Ducats = maps:get(<<"PrimePrice">>, Map, undefined),
    Credits = maps:get(<<"RegularPrice">>, Map, undefined),
    {price_part(Ducats), price_part(Credits)}.

price_part(V) when is_integer(V) -> io_lib:format("~p", [V]);
price_part(_) -> "".

format_hard(true) -> "hard";
format_hard(false) -> "";
format_hard(_) -> "".

sortie_variants(Map, Opts) ->
    Vars = maps:get(<<"Variants">>, Map, []),
    StageLabels = [sortie_stage_label(V, Opts) || V <- Vars],
    ModLabels = [sortie_modifier_label(V, Opts) || V <- Vars],
    {string:join([S || S <- StageLabels, S =/= ""], " | "),
     string:join([M || M <- ModLabels, M =/= ""], " | ")}.

sortie_stage_label(Var, Opts) when is_map(Var) ->
    Mission0 = maps:get(<<"missionType">>, Var, maps:get(<<"MissionType">>, Var, undefined)),
    Node0 = maps:get(<<"node">>, Var, maps:get(<<"Node">>, Var, undefined)),
    Mission = wfcli_resolve:resolve("missiontype", Mission0, Opts),
    case value_present(Node0) of
        true -> lists:flatten(io_lib:format("~s @ ~s", [Mission, wfcli_resolve:resolve("node", Node0, Opts)]));
        false -> Mission
    end;
sortie_stage_label(_, _Opts) -> "".

sortie_modifier_label(Var, Opts) when is_map(Var) ->
    Mod0 = maps:get(<<"modifierType">>, Var, maps:get(<<"Modifier">>, Var, undefined)),
    sortie_modifier_name(Mod0, Opts);
sortie_modifier_label(_, _Opts) -> "".

sortie_boss_name(undefined, _Opts) -> "";
sortie_boss_name(Val, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> to_list(Val);
        true ->
            Str = to_list(Val),
            case sortie_boss_map(Str) of
                undefined -> Str;
                Name -> Name
            end
    end.

sortie_boss_map("SORTIE_BOSS_BOREAL") -> "Boreal";
sortie_boss_map("SORTIE_BOSS_PHORID") -> "Phorid";
sortie_boss_map(_) -> undefined.

sortie_modifier_name(undefined, _Opts) -> "";
sortie_modifier_name(Val, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        false -> to_list(Val);
        true ->
            Str = to_list(Val),
            case sortie_modifier_map(Str) of
                undefined -> Str;
                Name -> Name
            end
    end.

sortie_modifier_map("SORTIE_MODIFIER_EXIMUS") -> "Eximus";
sortie_modifier_map("SORTIE_MODIFIER_FIRE") -> "Fire";
sortie_modifier_map("SORTIE_MODIFIER_RIFLE_ONLY") -> "Rifle Only";
sortie_modifier_map(_) -> undefined.

calendar_title(D, Opts) ->
    Season = season_name(maps:get(<<"Season">>, D, <<"Unknown">>), Opts),
    Day = to_list(maps:get(<<"Day">>, D, <<"Unknown">>)),
    lists:flatten(io_lib:format("Calendar: ~s day ~s", [Season, Day])).

calendar_events(D, Opts) ->
    Events = maps:get(<<"Events">>, D, []),
    case Events of
        [] -> "none";
        _ ->
            Parts = [calendar_event_summary(E, Opts) || E <- Events],
            string:join(Parts, " | ")
    end.

calendar_event_summary(Event, Opts) ->
    {Label, Value} = calendar_event_value(Event, Opts),
    case Value of
        "" -> Label;
        _ -> lists:flatten(io_lib:format("~s: ~s", [Label, Value]))
    end.

calendar_event_type(Event) ->
    Raw = to_list(maps:get(<<"type">>, Event, <<"Unknown">>)),
    case Raw of
        "CET_CHALLENGE" -> "Challenge";
        "CET_REWARD" -> "Reward";
        "CET_UPGRADE" -> "Upgrade";
        _ -> Raw
    end.

calendar_event_value(Event, Opts) ->
    case maps:get(<<"challenge">>, Event, undefined) of
        undefined ->
            case maps:get(<<"reward">>, Event, undefined) of
                undefined ->
                    case maps:get(<<"upgrade">>, Event, undefined) of
                        undefined -> {calendar_event_type(Event), ""};
                        Upgrade -> {calendar_event_type(Event), calendar_event_item(Upgrade, Opts)}
                    end;
                Reward -> {calendar_event_type(Event), calendar_event_item(Reward, Opts)}
            end;
        Challenge -> {calendar_event_type(Event), calendar_event_item(Challenge, Opts)}
    end.

calendar_event_item(Value, Opts) ->
    Raw = to_list(Value),
    case Raw of
        "" -> "";
        _ ->
            case maps:get(raw, Opts, false) orelse not maps:get(resolve_items, Opts, false) of
                true -> Raw;
                false ->
                    Pretty = wfcli_resolve:resolve("item", Value, Opts),
                    case Pretty =:= Raw of
                        true -> Raw;
                        false -> Pretty
                    end
            end
    end.

value_present(Value) ->
    retinue_text:value_present(Value).

expiry(Value) ->
    expiry(Value, #{}).

expiry(Value, Opts) ->
    expiry_value(Value, fun(Ms) -> retinue_time:format_millis(Ms, Opts) end).

circuit_category(<<"EXC_NORMAL">>) -> "Normal";
circuit_category(<<"EXC_HARD">>) -> "Steel Path";
circuit_category(Category) -> to_list(Category).

expiry_value(undefined, _FormatFun) -> "unknown";
expiry_value(#{<<"$date">> := #{<<"$numberLong">> := MsBin}}, FormatFun) ->
    expiry_value(MsBin, FormatFun);
expiry_value(Bin, FormatFun) when is_binary(Bin) ->
    expiry_value(binary_to_list(Bin), FormatFun);
expiry_value(Str, FormatFun) when is_list(Str) ->
    case string:to_integer(Str) of
        {Int, _} -> FormatFun(Int);
        _ -> Str
    end;
expiry_value(Int, FormatFun) when is_integer(Int) ->
    FormatFun(Int);
expiry_value(_, _FormatFun) -> "unknown".

to_list(V) -> retinue_text:to_list(V).

event_name(Map, Opts) ->
    case event_message(Map, event_lang(Opts)) of
        undefined -> "Event";
        Msg -> wfcli_resolve:resolve("item", Msg, Opts)
    end.

global_upgrade_name(Map, Opts) ->
    case first_value([<<"LocTag">>, <<"Desc">>, <<"Name">>, <<"Upgrade">>, <<"Operation">>], Map) of
        undefined -> "Global Upgrade";
        Val -> wfcli_resolve:resolve("item", Val, Opts)
    end.

syndicate_name(Map, Opts) ->
    case maps:get(<<"Tag">>, Map, undefined) of
        undefined -> "Syndicate Mission";
        Val -> wfcli_resolve:resolve("item", Val, Opts)
    end.

daily_deal_name(Map, Opts) ->
    case maps:get(<<"StoreItem">>, Map, undefined) of
        undefined -> "Daily Deal";
        Val -> wfcli_resolve:resolve("item", Val, Opts)
    end.

prime_vault_name(_Map, _Opts) ->
    "Prime Vault".

event_message(Map, Lang) ->
    Messages = maps:get(<<"Messages">>, Map, []),
    DefaultLang = case Lang of undefined -> <<"en">>; V -> V end,
    case pick_message(Messages, DefaultLang) of
        undefined -> pick_message(Messages, undefined);
        Msg -> Msg
    end.

event_lang(Opts) ->
    case maps:get(event_lang, Opts, undefined) of
        undefined -> undefined;
        Val -> to_bin(Val)
    end.

pick_message([], _) -> undefined;
pick_message([First | Rest], Lang) ->
    case {Lang, First} of
        {undefined, #{<<"Message">> := Msg}} -> Msg;
        {_, #{<<"LanguageCode">> := Lang, <<"Message">> := Msg}} -> Msg;
        _ -> pick_message(Rest, Lang)
    end.

event_prop(Map) ->
    case maps:get(<<"Prop">>, Map, undefined) of
        undefined -> "";
        <<>> -> "";
        Prop -> to_list(Prop)
    end.

first_date([], _Map, _Opts) -> undefined;
first_date([Key | Rest], Map, Opts) ->
    case maps:get(Key, Map, undefined) of
        undefined -> first_date(Rest, Map, Opts);
        Val -> expiry(Val, Opts)
    end.

window_value(Start, End) ->
    case {Start, End} of
        {undefined, undefined} -> "";
        {S, undefined} -> lists:flatten(io_lib:format("at ~s", [S]));
        {undefined, E} -> lists:flatten(io_lib:format("until ~s", [E]));
        {S, E} -> lists:flatten(io_lib:format("from ~s to ~s", [S, E]))
    end.

window_value_parts(Start, End) ->
    {window_value(Start, End), Start, End}.

window_from_dates_parts(StartRaw, EndRaw, Opts) ->
    Start = case StartRaw of undefined -> undefined; _ -> expiry(StartRaw, Opts) end,
    End = case EndRaw of undefined -> undefined; _ -> expiry(EndRaw, Opts) end,
    window_value_parts(Start, End).

event_window_parts(Map, Opts) ->
    Start = first_date([<<"EventStartDate">>, <<"Date">>, <<"Activation">>], Map, Opts),
    End = first_date([<<"EventEndDate">>, <<"Expiry">>], Map, Opts),
    window_value_parts(Start, End).

global_upgrade_window_parts(Map, Opts) ->
    Start = first_date([<<"Activation">>, <<"StartDate">>], Map, Opts),
    End = first_date([<<"Expiry">>, <<"EndDate">>], Map, Opts),
    window_value_parts(Start, End).

syndicate_window_parts(Map, Opts) ->
    Start = first_date([<<"Activation">>], Map, Opts),
    End = first_date([<<"Expiry">>], Map, Opts),
    window_value_parts(Start, End).

daily_deal_window_parts(Map, Opts) ->
    Start = first_date([<<"SaleStartDate">>, <<"Activation">>], Map, Opts),
    End = first_date([<<"SaleEndDate">>, <<"Expiry">>], Map, Opts),
    window_value_parts(Start, End).

prime_vault_window_parts(Map, Opts) ->
    Start = first_date([<<"Activation">>, <<"StartDate">>], Map, Opts),
    End = first_date([<<"Expiry">>, <<"EndDate">>], Map, Opts),
    window_value_parts(Start, End).

calendar_window_parts(Map, Opts) ->
    window_from_dates_parts(maps:get(<<"Activation">>, Map, undefined),
                            maps:get(<<"Expiry">>, Map, undefined), Opts).

global_upgrade_type(Map, Opts) ->
    case first_value([<<"Upgrade">>, <<"UpgradeType">>, <<"Type">>], Map) of
        undefined -> "";
        Val -> wfcli_resolve:resolve("item", Val, Opts)
    end.

syndicate_nodes(Map, Opts) ->
    Nodes = maps:get(<<"Nodes">>, Map, []),
    Pretty = [wfcli_resolve:resolve("node", N, Opts) || N <- Nodes],
    string:join([P || P <- Pretty, P =/= ""], ", ").

daily_deal_pricing(Map) ->
    Disc = maps:get(<<"Discount">>, Map, undefined),
    Orig = maps:get(<<"OriginalPrice">>, Map, undefined),
    Sale = maps:get(<<"SalePrice">>, Map, undefined),
    case {Disc, Orig, Sale} of
        {undefined, _, _} -> "";
        {_, undefined, _} -> "";
        {_, _, undefined} -> "";
        {D, O, S} ->
            lists:flatten(io_lib:format("~p% off (~p -> ~p)", [D, O, S]))
    end.

daily_deal_stock(Map) ->
    Total = maps:get(<<"AmountTotal">>, Map, undefined),
    Sold = maps:get(<<"AmountSold">>, Map, undefined),
    case {Total, Sold} of
        {undefined, _} -> "";
        {_, undefined} -> "";
        {T, S} ->
            Remaining = max(T - S, 0),
            lists:flatten(io_lib:format("~p/~p", [Remaining, T]))
    end.

prime_vault_items(Map, Opts) ->
    Items = maps:get(<<"Manifest">>, Map, []),
    Evergreen = maps:get(<<"EvergreenManifest">>, Map, []),
    CountStr =
        case {length(Items), length(Evergreen)} of
            {0, 0} -> "";
            {I, 0} -> lists:flatten(io_lib:format("~p", [I]));
            {I, E} -> lists:flatten(io_lib:format("~p (+ evergreen ~p)", [I, E]))
        end,
    Sample = prime_vault_sample(Items, Opts),
    case {CountStr, Sample} of
        {"", ""} -> "";
        {C, ""} -> C;
        {"", S} -> lists:flatten(io_lib:format("sample: ~s", [S]));
        {C, S} -> lists:flatten(io_lib:format("~s; sample: ~s", [C, S]))
    end.

prime_vault_sample([], _Opts) -> "";
prime_vault_sample(Items, Opts) ->
    Top = lists:sublist(Items, 5),
    Labels = [prime_vault_item_label(I, Opts) || I <- Top],
    Filtered = [L || L <- Labels, L =/= ""],
    case Filtered of
        [] -> "";
        _ -> lists:flatten(io_lib:format("~s", [string:join(Filtered, ", ")]))
    end.

prime_vault_item_label(#{<<"ItemType">> := Item} = Map, Opts) ->
    Name = wfcli_resolve:resolve("item", Item, Opts),
    Price = case {maps:get(<<"PrimePrice">>, Map, undefined), maps:get(<<"RegularPrice">>, Map, undefined)} of
        {P, _} when is_integer(P) -> lists:flatten(io_lib:format("~pP", [P]));
        {_, R} when is_integer(R) -> lists:flatten(io_lib:format("~pR", [R]));
        _ -> ""
    end,
    case Price of
        "" -> Name;
        _ -> lists:flatten(io_lib:format("~s (~s)", [Name, Price]))
    end;
prime_vault_item_label(_, _Opts) -> "".

void_trader_price(Map) ->
    Ducats = maps:get(<<"PrimePrice">>, Map, undefined),
    Credits = maps:get(<<"RegularPrice">>, Map, undefined),
    case {Ducats, Credits} of
        {undefined, undefined} -> "";
        {D, undefined} when is_integer(D) ->
            lists:flatten(io_lib:format("~p ducats", [D]));
        {undefined, C} when is_integer(C) ->
            lists:flatten(io_lib:format("~p credits", [C]));
        {D, C} when is_integer(D), is_integer(C) ->
            lists:flatten(io_lib:format("~p ducats / ~p credits", [D, C]));
        _ -> ""
    end.

mod_details_suffix(_ItemType, #{resolve_items := false}) -> "";
mod_details_suffix(undefined, _Opts) -> "";
mod_details_suffix(ItemType, _Opts) ->
    case mod_details(ItemType) of
        undefined -> "";
        Details ->
            mod_details_string(Details)
    end.

mod_details_string(#{rarity := Rarity, polarity := Pol, base_drain := Base, fusion_limit := Limit}) ->
    RarityStr = format_rarity(Rarity),
    PolStr = format_polarity(Pol),
    DrainStr = format_drain(Base, Limit),
    Parts = [P || P <- [RarityStr, PolStr, DrainStr], P =/= ""],
    string:join(Parts, ", ");
mod_details_string(_) -> "".

format_rarity(undefined) -> "";
format_rarity(Val) ->
    case string:lowercase(to_list(Val)) of
        "common" -> "Common";
        "uncommon" -> "Uncommon";
        "rare" -> "Rare";
        "legendary" -> "Legendary";
        Other -> Other
    end.

format_polarity(undefined) -> "";
format_polarity(Val) ->
    case mod_polarity_symbol(Val) of
        "" -> "";
        Sym -> lists:flatten(io_lib:format("pol ~s", [Sym]))
    end.

format_drain(Base, Limit) when is_integer(Base), is_integer(Limit) ->
    Max = Base + Limit,
    lists:flatten(io_lib:format("drain ~p->~p", [Base, Max]));
format_drain(Base, _) when is_integer(Base) ->
    lists:flatten(io_lib:format("drain ~p", [Base]));
format_drain(_, _) -> "".

mod_polarity_symbol(<<"AP_ATTACK">>) -> "V";
mod_polarity_symbol(<<"AP_DEFENSE">>) -> "D";
mod_polarity_symbol(<<"AP_TACTIC">>) -> "-";
mod_polarity_symbol(<<"AP_POWER">>) -> "=";
mod_polarity_symbol(<<"AP_WARD">>) -> "Y";
mod_polarity_symbol(<<"AP_UMBRA">>) -> "U";
mod_polarity_symbol(<<"AP_PRECEPT">>) -> "P";
mod_polarity_symbol(<<"AP_ANY">>) -> "";
mod_polarity_symbol(<<"AP_UNIVERSAL">>) -> "";
mod_polarity_symbol(Val) when is_binary(Val) -> mod_polarity_symbol(binary_to_list(Val));
mod_polarity_symbol(Val) when is_list(Val) -> mod_polarity_symbol(list_to_binary(Val));
mod_polarity_symbol(_) -> "".

first_value([], _) -> undefined;
first_value([Key | Rest], Map) ->
    case maps:get(Key, Map, undefined) of
        undefined -> first_value(Rest, Map);
        <<>> -> first_value(Rest, Map);
        Val -> Val
    end.

season_name(V, Opts) ->
    wfcli_resolve:resolve("season", V, Opts).

get_faction(Map, Key) ->
    case Key of
        attacker ->
            nonempty([
                maps:get(<<"AttackerFaction">>, Map, undefined),
                maps:get(<<"Faction">>, Map, undefined),
                get_in_map(maps:get(<<"AttackerMissionInfo">>, Map, #{}), <<"faction">>)
            ]);
        defender ->
            nonempty([
                maps:get(<<"DefenderFaction">>, Map, undefined),
                get_in_map(maps:get(<<"DefenderMissionInfo">>, Map, #{}), <<"faction">>)
            ]);
        _ ->
            nonempty([maps:get(Key, Map, undefined)])
    end.

nonempty([]) -> <<"">>;
nonempty([undefined | Rest]) -> nonempty(Rest);
nonempty([<<>> | Rest]) -> nonempty(Rest);
nonempty([Val | _]) -> Val.

get_in_map(Map, Key) when is_map(Map) -> maps:get(Key, Map, undefined);
get_in_map(_, _) -> undefined.


%%--------------------------------------------------------------------
%% Invasion reward helpers
%%--------------------------------------------------------------------
invasion_rewards(D, Opts) ->
    {fmt_reward(maps:get(<<"AttackerReward">>, D, #{}), Opts),
     fmt_reward(maps:get(<<"DefenderReward">>, D, #{}), Opts)}.

fmt_reward(R, Opts) when is_map(R) ->
    Item = reward_item(R, Opts),
    Credits = maps:get(<<"credits">>, R, 0),
    Parts = [fmt_part(P, Opts) || P <- maps:get(<<"countedItems">>, R, [])],
    Strings = lists:filter(fun(S) -> S =/= "" end, [Item, parts_join(Parts), credits_str(Credits)]),
    case Strings of
        [] -> "none";
        _ -> string:join(Strings, ", ")
    end;
fmt_reward(_, _) -> "none".

reward_item(R, Opts) ->
    wfcli_resolve:resolve("item", maps:get(<<"itemString">>, R, <<"">>), Opts).

fmt_part(#{<<"ItemType">> := Type, <<"ItemCount">> := Count}, Opts) ->
    lists:flatten(io_lib:format("~s x~p", [wfcli_resolve:resolve("item", Type, Opts), Count]));
fmt_part(_, _) -> "".

parts_join([]) -> "";
parts_join(List) -> string:join([P || P <- List, P =/= ""], " + ").

credits_str(0) -> "";
credits_str(N) when is_integer(N), N > 0 ->
    lists:flatten(io_lib:format("~pcr", [N]));
credits_str(_) -> "".

to_bin(V) ->
    retinue_text:to_binary(V).

-doc "Resolve one scalar optional field without moving presentation into the daemon.".
-spec resolve_extra_field(term(), term(), map()) -> {keep, term(), string()}.
resolve_extra_field(Key, V, Opts) ->
    {keep, Key, wfcli_resolve:resolve(extra_resolver_key(Key), V, Opts)}.

extra_resolver_key(Key) ->
    Normalized = string:lowercase(to_list(Key)),
    Resolvers = [{"faction", "faction"}, {"missiontype", "missiontype"},
                 {"modifier", "modifier"}, {"node", "node"},
                 {"season", "season"}, {"itemtype", "item"}],
    case [Resolver || {Suffix, Resolver} <- Resolvers,
                      lists:suffix(Suffix, Normalized)] of
        [Resolver | _] -> Resolver;
        [] -> Key
    end.

mod_details(ItemType) ->
    wfcli_mod_catalog:details(ItemType).
