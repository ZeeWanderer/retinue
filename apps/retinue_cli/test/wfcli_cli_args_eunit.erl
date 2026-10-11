-module(wfcli_cli_args_eunit).
-include_lib("eunit/include/eunit.hrl").

typed_aliases_and_repeated_values_test() ->
    Args = wfcli_test_cli:parse(["items", "-l7", "-o", "3", "-fjson",
                                 "--name", "Braton", "--name", "Prime"]),
    ?assertEqual(7, maps:get(limit, Args)),
    ?assertEqual(3, maps:get(offset, Args)),
    ?assertEqual(json, maps:get(output_format, Args)),
    ?assertMatch([#{vals := ["Braton"]}, #{vals := ["Prime"]}], maps:get(filters, Args)).

help_value_is_not_rewritten_test() ->
    Args = wfcli_test_cli:parse(["mods", "--name", "help"]),
    ?assertMatch([#{vals := ["help"]}], maps:get(filters, Args)),
    ?assertEqual(["help"], maps:get(query_tokens, wfcli_test_cli:parse(["query", "--search", "help"]))).

nested_help_test() ->
    lists:foreach(fun(Ending) ->
        ?assertEqual({help, ["baro", "inventory"]},
                     wfcli_test_cli:result(["baro", "inventory", Ending])),
        ?assertEqual({help, ["daemon", "start"]},
                     wfcli_test_cli:result(["daemon", "start", Ending])),
        ?assertEqual({help, ["forma-plan"]},
                     wfcli_test_cli:result(["forma-plan", Ending]))
    end, ["help", "--help", "-h"]),
    ?assertEqual({help, ["mods"]},
                 wfcli_test_cli:result(["mods", "--limit", "--help"])).

literal_arguments_test() ->
    Literal = ["--help", "-f", "--no-suggest-prompt", "help"],
    lists:foreach(fun(Command) ->
        Args = wfcli_test_cli:parse([Command, "--" | Literal]),
        ?assertEqual(Literal, maps:get(query_tokens, Args)),
        ?assertNot(maps:is_key(no_suggest_prompt, Args))
    end, ["query", "market", "mods", "items", "codex", "enemies", "drops", "fissures"]),
    Args = wfcli_test_cli:parse(["completion", "candidates", "--" | Literal]),
    ?assertEqual(Literal, maps:get(words, Args)).

query_order_survives_options_test() ->
    Args = wfcli_test_cli:parse(["query", "first", "--search", "second term",
                                 "--limit", "2", "third", "--", "--literal"]),
    ?assertEqual(["first", "second term", "third", "--literal"], maps:get(query_tokens, Args)).

global_output_flags_test() ->
    lists:foreach(fun(Args) ->
        ?assertMatch(#{output_format := json, utc := true}, wfcli_test_cli:parse(Args))
    end, [["--json", "--utc", "daemon", "status"],
          ["daemon", "--json", "status", "--utc"],
          ["query", "--utc", "dataset=captures", "--json"],
          ["--json", "items", "--utc"]]),
    ?assertMatch(#{output_format := table},
                 wfcli_test_cli:parse(["--json", "items", "--format", "table"])),
    ?assertMatch(#{output_format := json},
                 wfcli_test_cli:parse(["items", "--format", "table", "--json"])),
    ?assertMatch(#{output_format := table, query_tokens := ["--json", "--utc"]},
                 wfcli_test_cli:parse(["query", "--", "--json", "--utc"])).

global_prompt_flag_test() ->
    lists:foreach(fun(Args) ->
        ?assertEqual(true, maps:get(no_suggest_prompt, wfcli_test_cli:parse(Args)))
    end, [["--no-suggest-prompt", "mods"], ["mods", "--no-suggest-prompt"]]).

flags_do_not_consume_boolean_query_values_test() ->
    Args = wfcli_test_cli:parse(["mods", "--raw", "false"]),
    ?assertEqual(true, maps:get(raw, Args)),
    ?assertEqual(["false"], maps:get(query_tokens, Args)).

help_option_values_test() ->
    Help = unicode:characters_to_binary(wfcli_test_cli:help(["mods"])),
    ?assertNotEqual(nomatch, binary:match(Help, <<"--name <name>">>)),
    ?assertNotEqual(nomatch, binary:match(Help, <<"maximum results (int >= 0), default: 50">>)).

unknown_options_and_bad_values_test() ->
    lists:foreach(fun(Args) -> ?assertMatch({error, _, _, _}, wfcli_test_cli:result(Args)) end,
                  [["items", "--polarity", "V"], ["fissures", "--deep"],
                   ["update", "ignored-word"], ["daemon", "start", "--idle-timeout", "0"],
                   ["mods", "--limit", "7x"], ["fissures", "--ttl", "59"],
                   ["fissures", "--update-all"], ["mods", "--name"]]),
    {error, _, Error, _} = wfcli_test_cli:result(["query", "--formatt", "table"]),
    ?assertNotEqual(nomatch, string:find(Error, "did you mean")).

entire_tree_validates_and_has_help_test() ->
    Tree = wfcli_test_cli:tree(),
    ?assertEqual("wfcli", argparse:validate(Tree, #{progname => "wfcli"})),
    lists:foreach(fun(Path) ->
        ?assert(byte_size(unicode:characters_to_binary(wfcli_test_cli:help(Path))) > 20)
    end, paths(Tree, [])).

paths(Node, Path) ->
    [Path | lists:append([paths(Child, Path ++ [Name])
                         || {Name, Child} <- maps:to_list(maps:get(commands, Node, #{}))])].
