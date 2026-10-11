%%%-------------------------------------------------------------------
%% Common Test for shared table rendering.
%%%-------------------------------------------------------------------
-module(wfcli_table_SUITE).

-export([all/0,
         wrap_respects_width/1,
         status_color_applied/1,
         snapshot_simple_table/1,
         snapshot_complex_tables/1,
         complex_tables_respect_width/1,
         complex_mixed_tables_layout/1,
         uniform_columns_retained_when_space/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [wrap_respects_width,
     status_color_applied,
     snapshot_simple_table,
     snapshot_complex_tables,
     complex_tables_respect_width,
     complex_mixed_tables_layout,
     uniform_columns_retained_when_space].

wrap_respects_width(_Config) ->
    Headers = ["Name", "Effects"],
    Rows = [["Mod", "alpha beta gamma delta"]],
    Width = 20,
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => Width}
    ),
    lists:foreach(
      fun(Line) ->
          ?assert(retinue_tty:display_width(Line) =< Width)
      end,
      Lines).

status_color_applied(_Config) ->
    Headers = ["Name"],
    Rows = [["Mod"]],
    ColorFun = fun(Line, Status) ->
        case Status of
            add -> "ADD " ++ Line;
            _ -> Line
        end
    end,
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{mode => none, row_statuses => [add], status_color_fun => ColorFun}
    ),
    Body = lists:last(Lines),
    ?assert(lists:prefix("ADD ", Body)).

snapshot_simple_table(_Config) ->
    Output = render_simple_snapshot(),
    Expected = read_fixture("simple.txt"),
    ?assertEqual(Expected, Output).

snapshot_complex_tables(_Config) ->
    Output = render_complex_snapshot(),
    Expected = read_fixture("complex.txt"),
    ?assertEqual(Expected, Output).

complex_tables_respect_width(_Config) ->
    Tables = complex_tables(),
    lists:foreach(
      fun({Width, Lines}) ->
          lists:foreach(
            fun(Line) ->
                ?assert(retinue_tty:display_width(Line) =< Width)
            end,
            Lines)
      end,
      Tables).

complex_mixed_tables_layout(_Config) ->
    Headers = ["Type", "Name", "Details", "Note", "Unused"],
    Rows = [
        ["Fissure", "Lith Capture", "Short detail", "Core", ""],
        ["Fissure", "Lith Mobile Defense", "Another detail", "", ""],
        ["Alert", "Exterminate", "", "Bonus", ""],
        ["Alert", "Defense", "", "", ""],
        ["Event", "Void Tide", "Rotating missions with boosts", "Limited", ""]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    ?assert(lists:member("== Fissure ==", Lines)),
    ?assert(lists:member("== Alert ==", Lines)),
    ?assert(lists:member("== Event ==", Lines)),
    lists:foreach(
      fun(Line) ->
          ?assert(retinue_tty:display_width(Line) =< 80)
      end,
      Lines),
    AfterLabel = lists:nthtail(1, lists:dropwhile(fun(Line) -> Line =/= "== Fissure ==" end, Lines)),
    Header = hd(AfterLabel),
    ?assert(string:find(Header, "Type") =:= nomatch),
    ?assert(string:find(Header, "Unused") =:= nomatch).

uniform_columns_retained_when_space(_Config) ->
    Headers = ["Mission", "Level", "Faction", "Reward"],
    Rows = [
        ["Sabotage", "65-70", "Orokin", "Marks of Valiance x20, 12200cr"],
        ["Defense", "65-70", "Orokin", "Marks of Valiance x20, 5900cr"]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 120}),
    Header = hd(Lines),
    ?assert(string:find(Header, "Level") =/= nomatch),
    ?assert(string:find(Header, "Faction") =/= nomatch).

render_simple_snapshot() ->
    Headers = ["Name", "Type", "Effects"],
    Rows = [
        ["Vicious Bond", "SENTINEL",
         "Companion melee attacks strip 2.5% of enemy armor; spreads to nearby enemies within 1.5m."],
        ["Sepsis Claws", "MELEE",
         "+30% Toxin; +30% Status Chance; Converts all elemental damage to Toxin damage."]
    ],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 60}
    ),
    string:join(Lines, "\n") ++ "\n".

render_complex_snapshot() ->
    {T1Lines, T2Lines, T3Lines} = complex_tables_lines(),
    string:join(
      [
        "== Table 1 ==",
        string:join(T1Lines, "\n"),
        "",
        "== Table 2 ==",
        string:join(T2Lines, "\n"),
        "",
        "== Table 3 ==",
        string:join(T3Lines, "\n"),
        ""
      ],
      "\n").

complex_tables() ->
    {T1Lines, T2Lines, T3Lines} = complex_tables_lines(),
    [{80, T1Lines}, {64, T2Lines}, {90, T3Lines}].

complex_tables_lines() ->
    T1Headers = ["Type", "Name", "Effects", "Notes"],
    T1Rows = [
        ["Mod", "Aerial Bond",
         "Airborne kills decrease recovery time and create a cold field; stacks with duration.",
         "Companion"],
        ["Mod", "Momentous Bond",
         "Killing Eximus grants bonus elemental damage for 30s and reduces recovery time.",
         "Companion"],
        ["Event", "Void Tide", "Rotating missions with modifiers and boosted rewards.", "Limited time"]
    ],
    T1Lines = retinue_table:render_lines(
        T1Headers,
        T1Rows,
        #{width => 80}
    ),
    T2Headers = ["ID", "Summary", "Window"],
    T2Rows = [
        ["EVT-2025-12",
         "Cross-faction alerts with boosted void traces and bonus drops across the system.",
         "2025-12-28T04:00:00+02:00"],
        ["EVT-2025-13",
         "Limited-time bonus for archwing missions with special modifiers enabled.",
         "2025-12-28T06:00:00+02:00"]
    ],
    T2Lines = retinue_table:render_lines(
        T2Headers,
        T2Rows,
        #{width => 64}
    ),
    T3Headers = ["Category", "Location", "Details"],
    T3Rows = [
        ["Fissure", "Tiwaz (Void)", "DataPath:/Lotus/Types/Game/Projections/T4VoidProjectionEGold"],
        ["Alert", "Hepit (Void)", "Reward pack includes credits and relics; expires soon."],
        ["Sortie", "Phorid", "Assassination target with boosted armor and elemental resistances."]
    ],
    T3Lines = retinue_table:render_lines(
        T3Headers,
        T3Rows,
        #{width => 90}
    ),
    {T1Lines, T2Lines, T3Lines}.

read_fixture(Name) ->
    Path = filename:join([filename:dirname(?FILE), "fixtures", "table", Name]),
    {ok, Bin} = file:read_file(Path),
    binary_to_list(Bin).
