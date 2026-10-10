%%%-------------------------------------------------------------------
%% EUnit tests for terminal helper utilities.
%%%-------------------------------------------------------------------
-module(wfcli_tty_eunit).

-include_lib("eunit/include/eunit.hrl").

strip_ansi_test() ->
    ?assertEqual("Red", wfcli_tty:strip_ansi("\e[31mRed\e[0m")).

strip_terminal_control_test() ->
    ?assertEqual("Text", wfcli_tty:strip_ansi([io_ansi:clear(), "Text"])).

has_ansi_test() ->
    ?assert(wfcli_tty:has_ansi(io_ansi:green())),
    ?assertNot(wfcli_tty:has_ansi("Green")).

display_width_test() ->
    ?assertEqual(3, wfcli_tty:display_width("\e[31mRed\e[0m")).

display_width_unicode_test() ->
    ?assertEqual(2, wfcli_tty:display_width("界")).

colorize_test() ->
    Output = wfcli_tty:colorize("Green", green),
    ?assertEqual("Green", wfcli_tty:strip_ansi(Output)),
    ?assert(wfcli_tty:has_ansi(Output)).

colorize_dt_tags_test() ->
    Output = wfcli_tty:colorize_dt_tags("+15% <DT_POISON_COLOR>Toxin"),
    ?assert(re:run(Output, "\\x1b\\[[0-9;]*mToxin\\x1b\\[0m", []) =/= nomatch).

colorize_dt_tags_spacing_test() ->
    Output = wfcli_tty:colorize_dt_tags("+15% <DT_POISON_COLOR>Toxin damage"),
    ?assert(re:run(Output, "\\+15%\\s+\\x1b\\[", []) =/= nomatch).

colorize_dt_tags_semicolon_test() ->
    Output = wfcli_tty:colorize_dt_tags("+15% <DT_POISON_COLOR>Toxin;"),
    ?assert(re:run(Output, "Toxin\\x1b\\[0m;", []) =/= nomatch).

colorize_dt_tags_table_width_test() ->
    Plain = "+15% Toxin; +30% Toxin",
    Output = wfcli_tty:colorize_dt_tags(
      "+15% <DT_POISON_COLOR>Toxin; +30% <DT_POISON_COLOR>Toxin"),
    Width = wfcli_tty:display_width(Plain),
    [Line] = wfcli_table_text:wrap_cell(Output, Width, true),
    ?assertEqual(Plain, wfcli_tty:strip_ansi(Line)),
    ?assertEqual(Width, wfcli_tty:display_width(Line)),
    ?assertEqual(2, length(binary:matches(list_to_binary(Output), <<"\e[0m">>))).

colorize_special_tags_test() ->
    Output = wfcli_tty:colorize_dt_tags("Adds <LOWER_IS_BETTER>6 <ENERGY>"),
    ?assert(string:find(Output, "(lower is better) 6 Energy") =/= nomatch).

colorize_line_separator_test() ->
    Output = wfcli_tty:colorize_dt_tags("Alpha<LINE_SEPARATOR>Beta"),
    ?assert(string:find(Output, "Alpha\nBeta") =/= nomatch).

colorize_school_tags_test() ->
    Output = wfcli_tty:colorize_dt_tags("<MADURAI_CLEAN>"),
    ?assertEqual("Madurai", Output).

colorize_dt_tag_without_color_suffix_test() ->
    Output = wfcli_tty:colorize_dt_tags("+15% <DT_ELECTRICITY>Electricity"),
    ?assert(re:run(Output, "Electricity", []) =/= nomatch),
    ?assert(re:run(Output, "\\x1b\\[[0-9;]*mElectricity\\x1b\\[0m", []) =/= nomatch).

pad_right_test() ->
    ?assertEqual("Hi   ", wfcli_tty:pad_right("Hi", 5)).
