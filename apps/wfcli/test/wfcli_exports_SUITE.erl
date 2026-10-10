%%%-------------------------------------------------------------------
%% Common Test for export queries.
%%%-------------------------------------------------------------------
-module(wfcli_exports_SUITE).

-export([all/0,
         init_per_suite/1,
         end_per_suite/1,
         mods_query_filters/1,
         mods_query_negation/1,
           mods_query_range/1,
           mods_boolean_query/1,
         mods_effects_output/1,
         mods_raw_fields/1,
         mods_colorization/1,
         mods_raw_table/1,
         mods_format_alias/1,
         mods_header_order/1,
         items_query_filters/1,
         items_query_or/1,
         items_query_range/1,
         items_query_abilities/1,
         items_format_alias/1,
         items_json_output/1,
         items_weapon_slot/1,
         items_explicit_omitted_export/1,
         query_command_returns_results/1,
         items_large_offset_empty/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [mods_query_filters,
     mods_query_negation,
       mods_query_range,
       mods_boolean_query,
     mods_effects_output,
     mods_raw_fields,
     mods_colorization,
     mods_raw_table,
     mods_format_alias,
     mods_header_order,
     items_query_filters,
     items_query_or,
     items_query_range,
     items_query_abilities,
     items_format_alias,
     items_json_output,
     items_weapon_slot,
     items_explicit_omitted_export,
     query_command_returns_results,
     items_large_offset_empty].

init_per_suite(Config) ->
    {ok, _} = application:ensure_all_started(wfcli),
    ok = wfcli_test_daemon:start(),
    Config.

end_per_suite(_Config) ->
    wfcli_test_daemon:stop(),
    ok.

mods_query_filters(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "type:MELEE",
            "polarity:V",
            "toxin",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Venom Strike") =/= nomatch),
    ?assert(string:find(Output, "Frost Bite") =:= nomatch).

mods_query_negation(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "polarity!=V",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Frost Bite") =/= nomatch),
    ?assert(string:find(Output, "Venom Strike") =:= nomatch).

mods_query_range(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "basedrain>=6",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Venom Strike") =/= nomatch),
    ?assert(string:find(Output, "Frost Bite") =:= nomatch).

mods_boolean_query(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "(name~Venom OR name~Frost)",
            "polarity=V",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Venom Strike") =/= nomatch),
    ?assert(string:find(Output, "Frost Bite") =:= nomatch).

mods_effects_output(_Config) ->
    Dir = fixture_dir(),
    Output0 = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "name=Venom Strike",
            "--output-format", "table"
        ])
    end),
    Output = wfcli_tty:strip_ansi(Output0),
    ?assert(re:run(Output, "\\+15%\\s*Toxin", []) =/= nomatch),
    ?assert(re:run(Output, "\\+30%\\s*Toxin", []) =/= nomatch).

mods_raw_fields(_Config) ->
    Dir = fixture_dir(),
    {ok, Bin} = file:read_file(filename:join(Dir, "ExportUpgrades_en.json")),
    Expected = maps:get(<<"ExportUpgrades">>, jsone:decode(Bin, [{object_format, map}])),
    {ok, RawMods} = wfcli_exports:load_raw_items(Dir, "ExportUpgrades_en.json"),
    ?assertEqual(Expected, RawMods),
    [Raw] = [M || M <- RawMods, maps:get(<<"name">>, M) =:= <<"Venom Strike">>],
    {ok, Mods} = wfcli_exports:load_mods(Dir),
    [Mod] = [M || M <- Mods, maps:get(name, M) =:= "Venom Strike"],
    ?assertEqual(maps:get(<<"levelStats">>, Raw), maps:get(levelStats, Mod)),
    ?assertEqual(["+15% <DT_POISON_COLOR>Toxin", "+30% <DT_POISON_COLOR>Toxin"],
                 maps:get(effects, Mod)),
    ?assertEqual(["+30% <DT_POISON_COLOR>Toxin"], maps:get(max_stats, Mod)).

mods_colorization(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "name=Venom Strike",
            "--output-format", "table"
        ])
    end),
    ?assert(re:run(Output, "\\x1b\\[[0-9;]*mToxin\\x1b\\[0m", []) =/= nomatch).

mods_raw_table(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "name=Venom Strike",
            "--raw",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Unique") =/= nomatch),
    ?assert(string:find(Output, "/Lotus/Upgrades/Mods/Test/PoisonMod") =/= nomatch).

mods_format_alias(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "name=Venom Strike",
            "--format", "table"
        ])
    end),
    ?assert(string:find(Output, "Venom Strike") =/= nomatch).

mods_header_order(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "mods",
            "--exports-dir", Dir,
            "name=Venom Strike",
            "--output-format", "table"
        ])
    end),
    Header = find_header_line(Output, ["Name", "Effects"]),
    EffectsPos = string:find(Header, "Effects"),
    CompatPos = string:find(Header, "Compat"),
    ?assert(EffectsPos =/= nomatch),
    ?assert(CompatPos =/= nomatch),
    ?assert(EffectsPos > CompatPos).

items_query_filters(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "items",
            "--exports-dir", Dir,
            "--file", "ExportWeapons_en.json",
            "test",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Test Gun") =/= nomatch),
    ?assert(string:find(Output, "Alloy Sample") =:= nomatch).

items_query_or(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "items",
            "--exports-dir", Dir,
            "file=ExportWeapons_en.json|ExportResources_en.json",
            "sample",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Alloy Sample") =/= nomatch).

items_query_range(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "items",
            "--exports-dir", Dir,
            "masteryreq>=5",
            "file=ExportWeapons_en.json",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Test Gun") =/= nomatch).

items_query_abilities(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "items",
            "--exports-dir", Dir,
            "file=ExportWarframes_en.json",
            "abilities~test ability",
            "--output-format", "table"
        ])
    end),
    ?assert(string:find(Output, "Test Frame") =/= nomatch).

items_format_alias(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "items",
            "--exports-dir", Dir,
            "--file", "ExportWeapons_en.json",
            "test",
            "--format", "table"
        ])
    end),
    ?assert(string:find(Output, "Test Gun") =/= nomatch).

items_json_output(_Config) ->
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        wfcli_cli:main([
            "items",
            "--exports-dir", Dir,
            "--file", "ExportWeapons_en.json",
            "test",
            "--output-format", "json"
        ])
    end),
    ?assert(string:find(Output, "\"results\"") =/= nomatch),
    ?assert(string:find(Output, "Test Gun") =/= nomatch).

items_weapon_slot(_Config) ->
    {ok, [Item]} = wfcli_exports:load_items(fixture_dir(), ["ExportWeapons_en.json"]),
    ?assertEqual(0, maps:get(slot, Item)),
    ?assertEqual("/Lotus/Weapons/Test/TestGun", maps:get(uniqueName, Item)),
    ?assertEqual("Pistols", maps:get(productCategory, Item)).

items_explicit_omitted_export(_Config) ->
    Dir = fixture_dir(),
    {ok, _DefaultParsed, DefaultResults} = query_command("items", [
        "--exports-dir", Dir,
        "visible"
    ]),
    ?assertEqual(0, maps:get(total, DefaultResults)),
    {ok, _Parsed, Results} = query_command("items", [
        "--exports-dir", Dir,
        "--file", "ExportFlavour_en.json",
        "visible"
    ]),
    ?assertEqual(1, maps:get(total, Results)),
    [Entry] = maps:get(slice, Results),
    ?assertEqual("Visible Codex Entry", maps:get(name, maps:get(data, Entry))).

query_command_returns_results(_Config) ->
    Dir = fixture_dir(),
    {ok, _Parsed, Results} = query_command("mods", [
        "--exports-dir", Dir,
        "name=Venom Strike",
        "--limit", "1"
    ]),
    ?assertEqual(1, maps:get(total, Results)),
    ?assertEqual(1, maps:get(shown, Results)),
    [Entry] = maps:get(slice, Results),
    ?assertEqual("Venom Strike", maps:get(name, maps:get(data, Entry))).

items_large_offset_empty(_Config) ->
    Dir = fixture_dir(),
    {ok, _Parsed, Results} = query_command("items", [
        "--exports-dir", Dir,
        "test",
        "--offset", "99999"
    ]),
    ?assert(maps:get(total, Results) > 0),
    ?assertEqual(0, maps:get(shown, Results)),
    ?assertEqual([], maps:get(slice, Results)).

query_command(Command, Args) ->
    Query = wfcli_test_cli:catalog(Command, Args),
    wfcli_exports_query:query(Command, Query).

fixture_dir() ->
    filename:join([code:lib_dir(wfcli), "test", "fixtures", "exports"]).

capture_output(Fun) ->
    Capturer = spawn(fun() -> io_capture_loop([]) end),
    Old = group_leader(),
    group_leader(Capturer, self()),
    try
        _ = Fun()
    after
        group_leader(Old, self())
    end,
    Capturer ! {get, self()},
    receive
        {captured, Output} -> to_list(Output)
    after 1000 ->
        ""
    end.

io_capture_loop(Acc) ->
    receive
        {io_request, From, ReplyAs, Request} ->
            {NewAcc, Reply} = handle_io_request(Request, Acc),
            From ! {io_reply, ReplyAs, Reply},
            io_capture_loop(NewAcc);
        {get, Requestor} ->
            Requestor ! {captured, lists:flatten(lists:reverse(Acc))},
            io_capture_loop(Acc)
    end.

handle_io_request({put_chars, Chars}, Acc) ->
    {[Chars | Acc], ok};
handle_io_request({put_chars, _Enc, Chars}, Acc) ->
    {[Chars | Acc], ok};
handle_io_request({put_chars, Enc, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, Enc, apply(Mod, Fun, Args)}, Acc);
handle_io_request({put_chars, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, apply(Mod, Fun, Args)}, Acc);
handle_io_request({format, Format, Args}, Acc) ->
    {[io_lib:format(Format, Args) | Acc], ok};
handle_io_request({fwrite, Format, Args}, Acc) ->
    {[io_lib:format(Format, Args) | Acc], ok};
handle_io_request({requests, Reqs}, Acc) when is_list(Reqs) ->
    lists:foldl(
      fun(Req, {Acc0, _Reply0}) -> handle_io_request(Req, Acc0) end,
      {Acc, ok},
      Reqs);
handle_io_request(_Req, Acc) ->
    {Acc, ok}.

find_header_line(Output, Terms) ->
    Lines = string:split(Output, "\n", all),
    find_header_line(Lines, Terms, "").

find_header_line([], _Terms, Default) -> Default;
find_header_line([Line | Rest], Terms, Default) ->
    case lists:all(fun(T) -> string:find(Line, T) =/= nomatch end, Terms) of
        true -> Line;
        false -> find_header_line(Rest, Terms, Default)
    end.

to_list(V) when is_binary(V) -> binary_to_list(V);
to_list(V) when is_atom(V) -> atom_to_list(V);
to_list(V) when is_list(V) -> V;
to_list(V) -> io_lib:format("~p", [V]).
