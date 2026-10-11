%%%-------------------------------------------------------------------
%% Common Test for terminal helper utilities.
%%%-------------------------------------------------------------------
-module(retinue_tty_SUITE).

-export([all/0,
         strip_ansi/1,
         display_width/1,
         colorize_dt_tags/1,
         colorize_dt_tags_spacing/1,
         colorize_dt_tags_semicolon/1,
         colorize_special_tags/1,
         colorize_line_separator/1,
         colorize_school_tags/1,
         colorize_dt_tag_without_color_suffix/1,
         pad_right/1,
         terminal_width/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [strip_ansi,
     display_width,
     colorize_dt_tags,
     colorize_dt_tags_spacing,
     colorize_dt_tags_semicolon,
     colorize_special_tags,
     colorize_line_separator,
     colorize_school_tags,
     colorize_dt_tag_without_color_suffix,
     pad_right,
     terminal_width].

strip_ansi(_Config) ->
    ?assertEqual("Red", retinue_tty:strip_ansi("\e[31mRed\e[0m")).

display_width(_Config) ->
    ?assertEqual(3, retinue_tty:display_width("\e[31mRed\e[0m")).

colorize_dt_tags(_Config) ->
    Output = retinue_tty:colorize_dt_tags("+15% <DT_POISON_COLOR>Toxin"),
    ?assert(re:run(Output, "\\x1b\\[[0-9;]*mToxin\\x1b\\[0m", []) =/= nomatch).

colorize_dt_tags_spacing(_Config) ->
    Output = retinue_tty:colorize_dt_tags("+15% <DT_POISON_COLOR>Toxin damage"),
    ?assert(re:run(Output, "\\+15%\\s+\\x1b\\[", []) =/= nomatch).

colorize_dt_tags_semicolon(_Config) ->
    Output = retinue_tty:colorize_dt_tags("+15% <DT_POISON_COLOR>Toxin;"),
    ?assert(re:run(Output, "Toxin\\x1b\\[0m;", []) =/= nomatch).

colorize_special_tags(_Config) ->
    Output = retinue_tty:colorize_dt_tags("Adds <LOWER_IS_BETTER>6 <ENERGY>"),
    ?assert(string:find(Output, "(lower is better) 6 Energy") =/= nomatch).

colorize_line_separator(_Config) ->
    Output = retinue_tty:colorize_dt_tags("Alpha<LINE_SEPARATOR>Beta"),
    ?assert(string:find(Output, "Alpha\nBeta") =/= nomatch).

colorize_school_tags(_Config) ->
    Output = retinue_tty:colorize_dt_tags("<MADURAI_CLEAN>"),
    ?assertEqual("Madurai", Output).

colorize_dt_tag_without_color_suffix(_Config) ->
    Output = retinue_tty:colorize_dt_tags("+15% <DT_ELECTRICITY>Electricity"),
    ?assert(re:run(Output, "\\x1b\\[[0-9;]*mElectricity\\x1b\\[0m", []) =/= nomatch).

pad_right(_Config) ->
    ?assertEqual("Hi   ", retinue_tty:pad_right("Hi", 5)).

terminal_width(_Config) ->
    Width = retinue_tty:terminal_width(),
    ?assert(is_integer(Width)),
    ?assert(Width > 0).
