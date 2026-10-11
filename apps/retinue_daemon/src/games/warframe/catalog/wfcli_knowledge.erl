%%%-------------------------------------------------------------------
%% Static knowledge catalog assembled from official exports and WFCD data.
%%%-------------------------------------------------------------------
-module(wfcli_knowledge).

-export([load_codex/1, codex_sources/1,
         load_enemies/1, load_drops/1, wfcd_source/1,
         store_wfcd/2, default_wfcd_cache/0]).

-define(WFCD_FILE, "WFCDEnemy.json").

-type source_meta() :: #{source := string(), version := string(), fetched_at := integer()}.

-doc "Load and deduplicate Codex-visible records from official PublicExport files.".
-spec load_codex(file:filename_all() | undefined) -> {ok, [map()], map()}.
load_codex(ExportsDir) ->
    case wfcli_exports:load_items(ExportsDir, wfcli_worldstate:codex_export_files()) of
        {ok, Items} ->
            Entries = dedupe_codex(
                        [normalize_codex(Item) || Item <- Items,
                                                  maps:get(name, Item, "") =/= ""]),
            {ok, Entries, #{source => "official PublicExport", version => term_version(Entries)}};
        {error, Reason} -> {error, Reason}
    end.

-doc "Return official Codex source paths for daemon cache signatures.".
-spec codex_sources(file:filename_all() | undefined) -> [{string(), file:filename_all()}].
codex_sources(ExportsDir) ->
    wfcli_exports:item_sources(ExportsDir, wfcli_worldstate:codex_export_files()).

-doc "Load versioned WFCD enemy records from the local managed knowledge cache.".
-spec load_enemies(file:filename_all() | undefined) -> {ok, [map()], source_meta()} | {error, term()}.
load_enemies(KnowledgeDir) ->
    Path = wfcd_source(KnowledgeDir),
    case read_wrapper(Path) of
        {ok, Entries, Meta} ->
            Enemies = [normalize_enemy(Enemy, Meta) || Enemy <- Entries, is_map(Enemy)],
            {ok, dedupe_by(fun enemy_key/1, Enemies), Meta};
        Error -> Error
    end.

-doc "Flatten cached enemy drop lists into reverse-searchable item/enemy rows.".
-spec load_drops(file:filename_all() | undefined) -> {ok, [map()], source_meta()} | {error, term()}.
load_drops(KnowledgeDir) ->
    case load_enemies(KnowledgeDir) of
        {ok, Enemies, Meta} ->
            Drops = lists:flatten([enemy_drops(Enemy) || Enemy <- Enemies]),
            {ok, dedupe_by(fun drop_key/1, Drops), Meta};
        Error -> Error
    end.

-doc "Resolve the WFCD cache path; `KnowledgeDir` is used by tests and tools.".
-spec wfcd_source(file:filename_all() | undefined) -> file:filename_all().
wfcd_source(undefined) ->
    Paths = wfcli_worldstate:metadata_paths(?WFCD_FILE),
    pick_existing(Paths, default_wfcd_cache());
wfcd_source(KnowledgeDir) ->
    filename:join(KnowledgeDir, ?WFCD_FILE).

-doc "Return the preferred path used by `update --wfcd`.".
-spec default_wfcd_cache() -> file:filename_all().
default_wfcd_cache() ->
    case wfcli_worldstate:metadata_paths(?WFCD_FILE) of
        [Path | _] -> Path;
        [] -> filename:join(["apps", "retinue_daemon", "priv", ?WFCD_FILE])
    end.

-doc "Store validated WFCD enemy records with their snapshot provenance.".
-spec store_wfcd([map()], map()) -> ok | {error, term()}.
store_wfcd(Entries, Meta) ->
    Wrapper = Meta#{<<"entries">> => Entries},
    wfcli_worldstate:write_metadata_file(?WFCD_FILE, jsone:encode(Wrapper)).

content_version(Body) ->
    application:ensure_all_started(crypto),
    binary_to_list(binary:encode_hex(crypto:hash(sha256, Body), lowercase)).

term_version(Term) ->
    content_version(term_to_binary(Term, [deterministic])).

read_wrapper(Path) ->
    case file:read_file(Path) of
        {ok, Body} -> decode_wrapper(Body, Path);
        {error, enoent} -> {error, {knowledge_missing, Path}};
        {error, Reason} -> {error, {knowledge_read_failed, Path, Reason}}
    end.

decode_wrapper(Body, Path) ->
    try jsone:decode(Body, [{object_format, map}]) of
        #{<<"entries">> := Entries} = Wrapper when is_list(Entries) ->
            Meta = #{source => text(maps:get(<<"source">>, Wrapper, <<"WFCD warframe-items">>)),
                     version => text(maps:get(<<"version">>, Wrapper, <<"unknown">>)),
                     fetched_at => maps:get(<<"fetchedAt">>, Wrapper, 0)},
            {ok, Entries, Meta};
        _ -> {error, {bad_knowledge_cache, Path}}
    catch
        _:_ -> {error, {bad_knowledge_json, Path}}
    end.

normalize_codex(Item) ->
    Category = first_present([maps:get(productCategory, Item, undefined),
                              maps:get(category, Item, undefined), "Other"]),
    Item#{category => text(Category), source => "official PublicExport",
          sourceFiles => [maps:get(file, Item, "")]}.

dedupe_codex(Entries) ->
    Map = lists:foldl(fun merge_codex/2, #{}, Entries),
    maps:values(Map).

merge_codex(Item, Acc) ->
    Key = codex_key(Item),
    case maps:get(Key, Acc, undefined) of
        undefined -> Acc#{Key => Item};
        Existing -> Acc#{Key => merge_missing(Existing, Item)}
    end.

codex_key(Item) ->
    case maps:get(uniqueName, Item, "") of
        "" -> string:lowercase(maps:get(name, Item, "") ++ "|" ++ maps:get(category, Item, ""));
        Unique -> Unique
    end.

merge_missing(Preferred, Fallback) ->
    Merged = maps:fold(
      fun(Key, Value, Acc) ->
          case value_present(maps:get(Key, Acc, undefined)) of
              true -> Acc;
              false -> Acc#{Key => Value}
          end
      end,
      Preferred,
      Fallback),
    Files = unique(maps:get(sourceFiles, Preferred, []) ++ maps:get(sourceFiles, Fallback, [])),
    Merged#{sourceFiles => Files}.

normalize_enemy(Enemy, Meta) ->
    Drops = maps:get(<<"drops">>, Enemy, []),
    Resistances = maps:get(<<"resistances">>, Enemy, []),
    Type = text(maps:get(<<"type">>, Enemy, <<"">>)),
    Faction = text(first_present([maps:get(<<"faction">>, Enemy, undefined), Type])),
    #{name => text(maps:get(<<"name">>, Enemy, <<"Unknown">>)),
      uniqueName => text(maps:get(<<"uniqueName">>, Enemy, <<"">>)),
      description => text(maps:get(<<"description">>, Enemy, <<"">>)),
      faction => Faction,
      type => Type,
      health => number(maps:get(<<"health">>, Enemy, undefined)),
      shield => number(maps:get(<<"shield">>, Enemy, undefined)),
      armor => number(maps:get(<<"armor">>, Enemy, undefined)),
      dropCount => count_real_drops(Drops),
      drops => Drops,
      resistances => resistance_summary(Resistances),
      source => maps:get(source, Meta),
      sourceVersion => maps:get(version, Meta)}.

enemy_drops(Enemy) ->
    [normalize_drop(Enemy, Drop, Index)
     || {Index, Drop} <- lists:zip(lists:seq(1, length(maps:get(drops, Enemy, []))),
                                  maps:get(drops, Enemy, [])),
        is_map(Drop), is_number(maps:get(<<"chance">>, Drop, undefined)),
        text(maps:get(<<"location">>, Drop, <<"">>)) =/= ""].

normalize_drop(Enemy, Drop, Index) ->
    #{name => text(maps:get(<<"location">>, Drop, <<"Unknown">>)),
      item => text(maps:get(<<"location">>, Drop, <<"Unknown">>)),
      enemy => maps:get(name, Enemy),
      enemyUniqueName => maps:get(uniqueName, Enemy),
      table => text(maps:get(<<"type">>, Drop, <<"">>)),
      rarity => text(maps:get(<<"rarity">>, Drop, <<"">>)),
      chance => maps:get(<<"chance">>, Drop),
      index => Index,
      source => maps:get(source, Enemy),
      sourceVersion => maps:get(sourceVersion, Enemy)}.

enemy_key(Enemy) ->
    {maps:get(name, Enemy), maps:get(faction, Enemy), maps:get(health, Enemy),
     maps:get(shield, Enemy), maps:get(armor, Enemy), maps:get(resistances, Enemy),
     maps:get(description, Enemy), maps:get(drops, Enemy)}.

drop_key(Drop) ->
    {maps:get(item, Drop), maps:get(enemy, Drop), maps:get(chance, Drop),
     maps:get(rarity, Drop), maps:get(table, Drop)}.

dedupe_by(KeyFun, Entries) ->
    {_Seen, Reversed} = lists:foldl(
      fun(Entry, {Seen, Acc}) ->
          Key = KeyFun(Entry),
          case maps:is_key(Key, Seen) of
              true -> {Seen, Acc};
              false -> {Seen#{Key => true}, [Entry | Acc]}
          end
      end, {#{}, []}, Entries),
    lists:reverse(Reversed).

count_real_drops(Drops) when is_list(Drops) ->
    length([ok || Drop <- Drops, is_map(Drop), is_number(maps:get(<<"chance">>, Drop, undefined))]);
count_real_drops(_) -> 0.

resistance_summary(Resistances) when is_list(Resistances) ->
    string:join(unique([text(maps:get(<<"type">>, R, <<"">>))
                        || R <- Resistances, is_map(R),
                           text(maps:get(<<"type">>, R, <<"">>)) =/= ""]), ", ");
resistance_summary(_) -> "".

pick_existing([], Default) -> Default;
pick_existing([Path | Rest], Default) ->
    case filelib:is_file(Path) of
        true -> Path;
        false -> pick_existing(Rest, Default)
    end.

first_present([]) -> "";
first_present([Value | Rest]) ->
    case value_present(Value) of
        true -> Value;
        false -> first_present(Rest)
    end.

value_present(undefined) -> false;
value_present(null) -> false;
value_present("") -> false;
value_present(<<>>) -> false;
value_present(_) -> true.

number(Value) when is_integer(Value); is_float(Value) -> Value;
number(_) -> undefined.

unique(List) ->
    lists:reverse(element(2, lists:foldl(
      fun("", Acc) -> Acc;
         (Value, {Seen, Values}) ->
              case maps:is_key(Value, Seen) of
                  true -> {Seen, Values};
                  false -> {Seen#{Value => true}, [Value | Values]}
              end
      end,
      {#{}, []}, List))).

text(Value) when is_binary(Value) -> unicode:characters_to_list(Value);
text(Value) when is_atom(Value) -> atom_to_list(Value);
text(Value) when is_list(Value) -> Value;
text(Value) when is_integer(Value); is_float(Value) -> lists:flatten(io_lib:format("~p", [Value]));
text(_) -> "".
