-module(wf_worldstate_cli).

-export([commands/0, watch_command/0, run/1, prepare/1]).
-import(retinue_cli_args, [option/4, flag/3]).

commands() ->
    [{Name, command(Name, Type, Visible)} || {Name, Type, Visible} <- command_specs()].

command(Name, Type, Visible) ->
    Description = command_description(Name),
    Format = case Type of archimedea -> block; _ -> table end,
    Node = #{help => case Visible of true -> Description; false -> hidden end,
             summary => Description, handler => {?MODULE, run},
             defaults => #{type_filter => Type, mode => list, inventory => Type =:= teshin,
                           raw => false, watch => false, watch_always => false,
                           clear => false, once => false},
             arguments => retinue_cli_args:query() ++
                          retinue_cli_args:format([table, block], Format) ++
                          [flag(raw, "raw", "show raw identifiers")] ++
                          live_arguments(Type)},
    scope(Type, Node).

live_arguments(teshin) -> [];
live_arguments(_) ->
    [flag(refresh, "refresh", "refresh worldstate"),
     (option(ttl, "ttl", {integer, [{min, 60}]}, "cache freshness in seconds"))#{default => 60},
     option(cache, "cache", string, "worldstate cache file"),
     option(event_lang, "lang", string, "event language")] ++ watch_arguments().

watch_arguments() ->
    [(flag(watch, "watch", "watch for changes"))#{short => $w},
     (flag(diff, "diff", "watch and list differences"))#{short => $d},
     option(diff_style, "diff-style", {atom, [inline, list, diff, none]}, "watch diff style"),
     (flag(watch_always, "always", "print unchanged watch results"))#{short => $a},
     (option(interval, "interval", {integer, [{min, 60}]}, "watch interval in seconds"))#{
         default => 60},
     flag(clear, "clear", "clear terminal before each update"),
     (flag(clear, "no-clear", "retain previous terminal output"))#{action => {store, false}},
     flag(once, "once", "exit after the initial watch snapshot"),
     (option(spec_strings, "spec", string, "watch COMMAND[:QUERY] (repeatable)"))#{
         action => append}].

scope(Type, Node) when Type =:= baro; Type =:= prime_vault ->
    Node#{arguments := maps:get(arguments, Node) ++
                      [flag(inventory, "inventory", "show current inventory")],
          commands => #{"inventory" => #{help => "show current inventory",
                                         defaults => #{inventory => true},
                                         handler => {?MODULE, run}}}};
scope(calendar, Node) ->
    Node#{arguments := maps:get(arguments, Node) ++
                      [option(calendar_day, "day", {integer, [{min, 0}]}, "calendar day")]};
scope(archimedea, Node) ->
    Node#{arguments := maps:get(arguments, Node) ++
                      [selection_flag(deep), selection_flag(temporal)],
          commands => selections([{"deep", deep}, {"temporal", temporal}])};
scope(circuit, Node) ->
    Node#{notes => "\nNormal: Warframe rewards. Steel Path: Incarnon Genesis rewards.\n",
          commands => selections([{"normal", normal}, {"steel-path", steel_path}])};
scope(_Type, Node) -> Node.

selection_flag(Name) ->
    (flag(selections, atom_to_list(Name), "select " ++ atom_to_list(Name)))#{
        action => {append, Name}}.

selections(Pairs) ->
    maps:from_list([{Name, #{help => "show " ++ Name, handler => {?MODULE, run},
                            defaults => #{selection => Value}}} || {Name, Value} <- Pairs]).

watch_command() ->
    Node = command("watch", undefined, true),
    Node#{defaults := (maps:get(defaults, Node))#{watch => true},
          arguments := [Arg || Arg <- maps:get(arguments, Node),
                               maps:get(name, Arg) =/= query_tokens] ++
                       [#{name => spec_strings, nargs => list, action => extend,
                          required => false, default => [], help => "COMMAND[:QUERY]"},
                        #{name => spec_strings, long => "-", nargs => all,
                          action => extend, help => hidden}]}.

run(Args) ->
    case prepare(Args) of
        {error, Message} -> retinue_cli_runtime:fail(Message);
        {ok, #{watch := true} = Parsed} -> wf_worldstate_watch_cli:run(Parsed);
        {ok, Parsed} -> run_daemon_once(Parsed)
    end.

prepare(#{type_filter := circuit, selection := _, query_tokens := [Other | _]})
  when Other =:= "normal"; Other =:= "steel-path" ->
    {error, "normal and steel-path are mutually exclusive"};
prepare(Args) ->
    Tokens = maps:get(query_tokens, Args, []),
    Search = case Tokens of [] -> undefined; _ -> string:join(Tokens, " ") end,
    Watch = maps:get(watch, Args, false) orelse maps:get(diff, Args, false)
            orelse maps:get(watch_always, Args, false) orelse maps:is_key(spec_strings, Args),
    %% An explicit diff style enables watching; the default comes from this handler.
    ExplicitWatch = Watch orelse maps:is_key(diff_style, Args),
    Style = case maps:get(diff, Args, false) of
                true -> list;
                false -> maps:get(diff_style, Args, inline)
            end,
    Parsed = Args#{search => Search, resolve_items => not maps:get(raw, Args, false),
                   watch => ExplicitWatch, diff_style => Style, watch_specs => []},
    Selected = lists:usort(maps:get(selections, Args, []) ++
                           case maps:find(selection, Args) of
                               {ok, Selection} -> [Selection]; error -> []
                           end),
    case Selected of
        [] -> prepare_watch(Parsed);
        [Only] -> prepare_watch(add_search_clause(selection_query(Only), Parsed));
        _ -> {error, "scope selectors are mutually exclusive"}
    end.

selection_query(deep) -> "archimedea=deep";
selection_query(temporal) -> "archimedea=temporal";
selection_query(normal) -> "data.Category=EXC_NORMAL";
selection_query(steel_path) -> "data.Category=EXC_HARD".

prepare_watch(#{watch := true, inventory := true}) ->
    {error, "inventory cannot be combined with watch"};
prepare_watch(Parsed) ->
    case parse_spec_strings(maps:get(spec_strings, Parsed, []), []) of
        {ok, Specs} -> {ok, Parsed#{watch_specs := Specs}};
        {error, _} = Error -> Error
    end.

parse_spec_strings([], Acc) -> {ok, Acc};
parse_spec_strings([Text | Rest], Acc) ->
    case parse_watch_spec(Text) of
        {ok, Specs} -> parse_spec_strings(Rest, lists:reverse(Specs) ++ Acc);
        {error, _} = Error -> Error
    end.

add_search_clause(Clause, #{search := undefined} = Args) -> Args#{search := Clause};
add_search_clause(Clause, #{search := Search} = Args) ->
    Args#{search := "(" ++ Search ++ ") " ++ Clause}.

run_daemon_once(Parsed) ->
    case inventory_type(Parsed) of
        {error, Msg} ->
            io:format(standard_error, "error: ~s~n", [Msg]),
            halt(1);
        Inventory ->
            Request = #{source => request_source(Inventory),
                        opts => wf_worldstate_output:load_opts(Parsed),
                        query => maps:get(search, Parsed, undefined),
                        type_filter => maps:get(type_filter, Parsed, undefined),
                        day_filter => maps:get(calendar_day, Parsed, undefined),
                        mode => maps:get(mode, Parsed, list),
                        inventory => Inventory},
            case retinue_client:one_shot(warframe, Request) of
                {ok, Result} -> wf_worldstate_output:print_daemon_result(Result, Parsed);
                {error, Reason} ->
                    io:format(standard_error, "worldstate daemon error: ~ts~n",
                              [retinue_client:format_error(Reason)]),
                    halt(1)
            end
    end.

inventory_type(Parsed) ->
    case maps:get(inventory, Parsed, false) of
        false -> false;
        true ->
            case maps:get(type_filter, Parsed, undefined) of
                baro -> baro;
                prime_vault -> prime_vault;
                teshin -> teshin;
                _ -> {error, "--inventory only applies to baro, prime-vault, or teshin"}
            end
    end.

request_source(teshin) -> teshin;
request_source(_) -> worldstate.

command_description("baro") -> "show Baro schedule, relay, or current inventory";
command_description("teshin") -> "show current Teshin Steel Path inventory";
command_description("prime-vault") -> "show Prime Vault schedule or inventory";
command_description("calendar") -> "show calendar season schedule";
command_description("arbitration") -> "show current Arbitration";
command_description("archimedea") -> "show current Deep and Temporal Archimedea rotations";
command_description(Command) when Command =:= "circuit"; Command =:= "endless-xp" ->
    "show this week's Normal and Steel Path Circuit rewards";
command_description("sorties") -> "show current Sortie";
command_description("watch") -> "watch one or more data commands";
command_description(Name) ->
    "list " ++ lists:flatten(string:replace(Name, "-", " ", all)).

command_specs() ->
    [
        {"invasions", invasion, true},
        {"invasion", invasion, false},
        {"fissures", fissure, true},
        {"sorties", sortie, true},
        {"alerts", alert, true},
        {"alert", alert, false},
        {"baro", baro, true},
        {"teshin", teshin, true},
        {"arbitration", arbitration, true},
        {"arbitrations", arbitration, false},
        {"voidstorms", void_storm, true},
        {"events", event, true},
        {"calendar", calendar, true},
        {"global-upgrades", global_upgrade, true},
        {"syndicate-missions", syndicate_mission, true},
        {"daily-deals", daily_deal, true},
        {"prime-vault", prime_vault, true},
        {"flash-sales", flash_sale, true},
        {"goals", goal, true},
        {"archimedea", archimedea, true},
        {"conquests", archimedea, false},
        {"construction-projects", construction_project, true},
        {"descents", descent, true},
        {"circuit", circuit, true},
        {"endless-xp", circuit, false},
        {"experiment-recommended", experiment_recommended, true},
        {"featured-guilds", featured_guild, true},
        {"hub-events", hub_event, true},
        {"in-game-market", in_game_market, true},
        {"library", library_info, true},
        {"lite-sorties", lite_sortie, true},
        {"node-overrides", node_override, true},
        {"pvp-active-tournaments", pvp_active_tournament, true},
        {"pvp-alternative-modes", pvp_alternative_mode, true},
        {"pvp-challenges", pvp_challenge, true},
        {"persistent-enemies", persistent_enemy, true},
        {"prime-access", prime_access, true},
        {"prime-token", prime_token, true},
        {"prime-vault-availabilities", prime_vault_availability, true},
        {"project-pct", project_pct, true},
        {"season-info", season_info, true},
        {"sku-sales", sku_sale, true},
        {"twitch-promos", twitch_promo, true},
        {"meta", meta, true}
    ].

command_type(Name) ->
    case lists:keyfind(Name, 1, command_specs()) of
        {Name, Type, _Show} -> {ok, Type};
        false -> error
    end.

parse_watch_spec(Spec) ->
    Spec1 = string:trim(Spec),
    case Spec1 of
        "" -> {error, "watch spec cannot be empty"};
        _ ->
            Specs = split_watch_group(Spec1),
            parse_watch_specs_list(Specs, [])
    end.

parse_watch_specs_list([], Acc) ->
    {ok, lists:reverse(Acc)};
parse_watch_specs_list([Spec | Rest], Acc) ->
    {Name0, Query0} = split_watch_spec(Spec),
    Name = string:lowercase(string:trim(Name0)),
    Query1 = string:trim(Query0),
    Query = case Query1 of "" -> undefined; _ -> Query1 end,
      case watch_type_filter(Name) of
          {ok, Type} ->
              Label = case Query of
                  undefined -> Name;
                  _ -> Name ++ " (" ++ Query ++ ")"
              end,
              parse_watch_specs_list(
                Rest, [#{label => Label, type_filter => Type, query => Query} | Acc]);
          {error, Msg} -> {error, Msg}
      end.

split_watch_group(Spec) ->
    case has_query_operators(Spec) of
        true -> [Spec];
        false -> string:split(Spec, "|", all)
    end.

has_query_operators(Spec) ->
    lists:any(fun(S) -> string:find(Spec, S) =/= nomatch end, [":", "=", "~", ">", "<"]).

split_watch_spec(Spec) ->
    case string:split(Spec, ":", leading) of
        [Name, Query] -> {Name, Query};
        [_] ->
            case string:split(Spec, "=", leading) of
                [Name1, Query1] -> {Name1, Query1};
                [_] -> {Spec, ""}
            end
    end.

watch_type_filter(Name) ->
    case command_type(Name) of
        {ok, Type} -> {ok, Type};
        error -> {error, io_lib:format("unknown watch spec: ~s", [Name])}
    end.
