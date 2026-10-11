%%%-------------------------------------------------------------------
%% EUnit tests for shared table rendering.
%%%-------------------------------------------------------------------
-module(wfcli_table_eunit).

-include_lib("eunit/include/eunit.hrl").

header_cells([Header | _]) ->
    [string:trim(C) || C <- re:split(Header, "\\s{2,}", [{return, list}])];

header_cells([]) -> [].

rand_seed() ->
    _ = rand:seed(exsplus, {11, 22, 33}),
    ok.

rand_int(Min, Max) ->
    Min + rand:uniform(Max - Min + 1) - 1.

rand_pick(List) ->
    lists:nth(rand_int(1, length(List)), List).

rand_word() ->
    rand_pick(["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf"]).

rand_cell() ->
    Base = rand_word(),
    case rand_int(1, 5) of
        1 -> Base;
        2 -> Base ++ " " ++ rand_word();
        3 -> Base ++ "/" ++ rand_word() ++ "/" ++ rand_word();
        4 -> "\e[31m" ++ Base ++ "\e[0m";
        _ -> unicode_cell()
    end.

unicode_cell() ->
    Case = rand_int(1, 4),
    case Case of
        1 -> "界";
        2 -> "Ω";
        3 -> "Alpha — Beta";
        _ -> "Тест"
    end.

rand_table() ->
    Cols = rand_int(3, 6),
    Headers = [lists:nth(I, ["Name", "Summary", "Details", "Note", "Extra", "Path"]) || I <- lists:seq(1, Cols)],
    Rows = [ [rand_cell() || _ <- lists:seq(1, Cols)] || _ <- lists:seq(1, rand_int(2, 6)) ],
    {Headers, Rows}.

render_lines_wrap_test() ->
    Headers = ["Name", "Effects"],
    Rows = [["Mod", "alpha beta gamma delta"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 20}
    ),
    BodyLines = lists:nthtail(2, Lines),
    ?assert(length(BodyLines) > 1).

render_lines_status_color_test() ->
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

render_lines_drop_columns_test() ->
    Headers = ["A", "B", "C"],
    Rows = [["one", "two", "three"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 6}
    ),
    Header = hd(Lines),
    ?assert(string:find(Header, "B") =:= nomatch),
    ?assert(string:find(Header, "C") =:= nomatch).

render_lines_drop_empty_columns_test() ->
    Headers = ["Name", "Empty", "Value"],
    Rows = [["A", "", "1"], ["B", "", "2"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 80}
    ),
    Cells = header_cells(Lines),
    ?assert(lists:member("Name", Cells)),
    ?assert(lists:member("Value", Cells)),
    ?assertEqual(false, lists:member("Empty", Cells)).

render_lines_drop_uniform_column_multirow_test() ->
    Headers = ["Type", "Name"],
    Rows = [["Fissure", "A"], ["Fissure", "B"]],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 8}),
    Cells = header_cells(Lines),
    ?assertEqual(false, lists:member("Type", Cells)),
    ?assert(lists:member("Name", Cells)).

render_lines_drop_uniform_column_single_row_test() ->
    Headers = ["Type", "Name"],
    Rows = [["Fissure", "A"]],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 40}),
    Cells = header_cells(Lines),
    ?assertEqual(false, lists:member("Type", Cells)),
    ?assert(lists:member("Name", Cells)).

render_lines_respects_column_roles_test() ->
    Headers = ["Name", "Details", "Notes"],
    Rows = [["Alpha", "very long details text", "extra"]],
    Specs = [
        #{label => "Name", role => name},
        #{label => "Details", role => details, optional => true},
        #{label => "Notes", role => details, optional => true}
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 8, column_specs => Specs}),
    Cells = header_cells(Lines),
    ?assert(lists:member("Name", Cells)).

render_lines_parts_keep_nonempty_column_test() ->
    Headers = ["Window", "Name"],
    Rows = [["", "A"], ["", "B"]],
    RowMaps = [
        #{window_start => "2024-01-01", window_end => "2024-01-02"},
        #{window_start => "2024-02-01", window_end => "2024-02-02"}
    ],
    Specs = [
        #{label => "Window", role => time, kind => time_range, source => {row_map, [window_start, window_end]}},
        #{label => "Name", role => name}
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 40, column_specs => Specs, row_maps => RowMaps}),
    Cells = header_cells(Lines),
    ?assert(lists:member("Window", Cells)).

render_lines_drop_duplicate_column_test() ->
    Headers = ["Name", "ItemType", "Price"],
    Rows = [
        ["Alpha", "Alpha", "1"],
        ["Bravo", "Bravo", "2"]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    Cells = header_cells(Lines),
    ?assert(lists:member("Name", Cells)),
    ?assertEqual(false, lists:member("ItemType", Cells)),
    ?assert(lists:member("Price", Cells)).

render_lines_drop_duplicate_active_tier_test() ->
    Headers = ["Tier", "ActiveMissionTier", "Node"],
    Rows = [
        ["Axi", "Axi", "Pluto"],
        ["Lith", "Lith", "Earth"]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    Cells = header_cells(Lines),
    ?assert(lists:member("Tier", Cells)),
    ?assertEqual(false, lists:member("ActiveMissionTier", Cells)),
    ?assert(lists:member("Node", Cells)).

render_lines_group_resolver_test() ->
    Headers = ["Type", "Summary", "Details"],
    Rows = [["Alert", "A", ""], ["Fissure", "B", "detail"]],
    Specs = [
        #{label => "Type", role => type},
        #{label => "Summary", role => name},
        #{label => "Details", role => details, optional => true}
    ],
    Resolver = fun(Key, _Rows, _Maps, Opts) ->
        case Key of
            "Alert" -> {["Mission"], [["Rescue"]], Opts};
            "Fissure" -> {["Tier"], [["Lith"]], Opts};
            _ -> {[], [], Opts}
        end
    end,
    Lines = retinue_table:render_lines(Headers, Rows, #{
        width => 40,
        column_specs => Specs,
        group_resolver => Resolver
    }),
    ?assert(lists:any(fun(Line) -> string:find(Line, "Mission") =/= nomatch end, Lines)),
    ?assert(lists:any(fun(Line) -> string:find(Line, "Tier") =/= nomatch end, Lines)).

render_lines_sanitize_newlines_test() ->
    Headers = ["Name", "Details"],
    Rows = [["Gauss Prime Theme\n\"Redline\"", "Line1\r\nLine2\tTabbed"]],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 40}),
    ?assert(lists:all(fun(Line) -> string:find(Line, "\n") =:= nomatch end, Lines)),
    ?assert(lists:all(fun(Line) -> string:find(Line, "\r") =:= nomatch end, Lines)).

render_lines_sparse_columns_move_right_test() ->
    Headers = ["A", "B", "C", "D"],
    Rows = [
        ["a1", "b1", "c1", ""],
        ["a2", "", "c2", ""],
        ["a3", "", "c3", "d3"]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    Cells = header_cells(Lines),
    ?assertEqual(["A", "C", "B", "D"], Cells).

render_lines_does_not_expand_when_fit_test() ->
    Headers = ["A", "B"],
    Rows = [["one", "two"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 20}
    ),
    Header = hd(Lines),
    ?assertEqual(8, retinue_tty:display_width(Header)).

render_lines_wrap_unicode_width_test() ->
    Headers = ["Col"],
    Rows = [["\x{1100}\x{1100}\x{1100}"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 4}
    ),
    BodyLines = lists:nthtail(2, Lines),
    ?assert(lists:all(fun(Line) -> retinue_tty:display_width(Line) =< 4 end, BodyLines)).

render_lines_wrap_slash_split_test() ->
    Headers = ["Details"],
    Rows = [["/Lotus/Types/Challenges/Calendar1999/CalendarKillTechrotEnemiesEasy"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 25}
    ),
    BodyLines = lists:nthtail(2, Lines),
    ?assert(lists:any(fun(Line) -> string:find(Line, "CalendarKillTec") =/= nomatch end, BodyLines)).

render_lines_wrap_iso_timestamp_split_test() ->
    Headers = ["Window"],
    Rows = [["from 2025-12-22T02:00:00+02:00 to 2025-12-29T02:00:00+02:00"]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 16}
    ),
    BodyLines = lists:nthtail(2, Lines),
    ?assert(lists:any(fun(Line) -> string:find(Line, "2025-12-22") =/= nomatch end, BodyLines)),
    ?assert(lists:any(fun(Line) -> string:find(Line, "02:00:00+02:00") =/= nomatch end, BodyLines)).

render_lines_auto_split_by_type_test() ->
    Headers = ["Type", "Name", "Details"],
    Rows = [
        ["Mod", "Aerial Bond", "Companion bonuses"],
        ["Event", "Void Tide", ""],
        ["Mod", "Momentous Bond", "Damage buffs"]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    ?assert(lists:member("== Mod ==", Lines)),
    ?assert(lists:member("== Event ==", Lines)),
    AfterLabel = lists:nthtail(1, lists:dropwhile(fun(Line) -> Line =/= "== Mod ==" end, Lines)),
    Header = hd(AfterLabel),
    ?assert(string:find(Header, "Type") =:= nomatch),
    ?assert(string:find(Header, "Name") =/= nomatch),
    ?assert(string:find(Header, "Details") =/= nomatch).

render_lines_wrap_ansi_reset_test() ->
    Headers = ["Effects"],
    RedWord = "\e[38;2;255;0;0mRedTextLong\e[0m",
    Rows = [[RedWord]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 6}
    ),
    Body = lists:nthtail(2, Lines),
    ?assert(lists:any(fun(Line) -> string:find(Line, "\e[0m") =/= nomatch end, Body)).

render_lines_drop_uniform_type_column_test() ->
    Headers = ["Type", "Name"],
    Rows = [
        ["Alert", "Alpha"],
        ["Alert", "Bravo"]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    HeaderLine = hd(Lines),
    ?assert(string:find(HeaderLine, "Type") =:= nomatch),
    ?assert(string:find(HeaderLine, "Name") =/= nomatch).

render_lines_wrap_ansi_reemit_single_color_test() ->
    Headers = ["Effects"],
    RedWord = "\e[31mRedTextLongWord\e[0m",
    Rows = [[RedWord]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 6}
    ),
    Body = lists:nthtail(2, Lines),
    ?assert(lists:all(fun(Line) -> string:find(Line, "\e[31m") =/= nomatch end, Body)).

render_lines_wrap_ansi_reemit_multi_color_test() ->
    Headers = ["Effects"],
    RedPart = "\e[31mRedSegment\e[0m",
    BluePart = "\e[34mBlueSegment\e[0m",
    Rows = [[RedPart ++ " " ++ BluePart]],
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        #{width => 8}
    ),
    Body = lists:nthtail(2, Lines),
    lists:foreach(
      fun(Line) ->
          Stripped = retinue_tty:strip_ansi(Line),
          case string:find(Stripped, "Red") of
              nomatch -> ok;
              _ -> ?assert(string:find(Line, "\e[31m") =/= nomatch)
          end,
          case string:find(Stripped, "Blue") of
              nomatch -> ok;
              _ -> ?assert(string:find(Line, "\e[34m") =/= nomatch)
          end
      end,
      Body).

render_lines_fuzz_tables_test() ->
    rand_seed(),
    Width = 60,
    lists:foreach(
      fun(_Idx) ->
          {Headers, Rows} = rand_table(),
          Lines = retinue_table:render_lines(Headers, Rows, #{width => Width}),
          ?assert(length(Lines) >= 2),
          lists:foreach(fun(Line) ->
              ?assert(retinue_tty:display_width(Line) =< Width)
          end, Lines)
      end,
      lists:seq(1, 20)).

render_lines_auto_split_fixed_test() ->
    Headers = ["Type", "Name", "Details", "Note"],
    Rows = [
        ["A", "Alpha", "Has details", ""],
        ["B", "Bravo", "", ""],
        ["A", "Aria", "More details", ""]
    ],
    Lines = retinue_table:render_lines(Headers, Rows, #{width => 80}),
    ?assert(lists:member("== A ==", Lines)),
    ?assert(lists:member("== B ==", Lines)).

render_lines_ansi_unicode_width_test() ->
    Headers = ["Name", "Details"],
    Rows = [
        ["\e[32mGreen界\e[0m", "Alpha — Beta / Gamma"],
        ["Ωmega", "Path/To/Thing"]
    ],
    Width = 24,
    Lines = retinue_table:render_lines(Headers, Rows, #{width => Width}),
    lists:foreach(
      fun(Line) ->
          ?assert(retinue_tty:display_width(Line) =< Width)
      end,
      Lines).

render_lines_fuzz_varying_widths_test() ->
    rand_seed(),
    lists:foreach(
      fun(_Idx) ->
          {Headers, Rows} = rand_table(),
          Width = rand_int(30, 90),
          Lines = retinue_table:render_lines(Headers, Rows, #{width => Width}),
          ?assert(length(Lines) >= 2),
          lists:foreach(
            fun(Line) ->
                ?assert(retinue_tty:display_width(Line) =< Width)
            end,
            Lines)
      end,
      lists:seq(1, 20)).
