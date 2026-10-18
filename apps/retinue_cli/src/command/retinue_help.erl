-module(retinue_help).

-export([command/0, command/1, run/1, text/1]).

command() -> command([]).

command(Prefix) ->
    #{help => "show command help", handler => fun(Args) -> run(Prefix ++ maps:get(topic, Args, [])) end,
      arguments => [#{name => topic, nargs => all, required => false, default => [],
                      help => "command path or query-syntax"}]}.

run(Topic) ->
    case valid_topic(Topic) of
        true -> retinue_output:emit(#{help => unicode:characters_to_binary(text(Topic))},
                                  fun() -> io:put_chars(text(Topic)) end);
        false -> retinue_cli_runtime:fail(["unknown help topic: ", lists:join(" ", Topic)])
    end.

valid_topic(["query-syntax"]) -> true;
valid_topic(["warframe", "query-syntax"]) -> true;
valid_topic(["commands"]) -> true;
valid_topic(["--help"]) -> true;
valid_topic(["-h"]) -> true;
valid_topic([Scope, Alias]) when Alias =:= "commands"; Alias =:= "--help"; Alias =:= "-h" ->
    valid_topic([Scope]);
valid_topic(Topic) ->
    try retinue_cli_args:node(Topic, maps:get(tree, retinue_cli_runtime:context())) of
        _ -> true
    catch error:{badkey, _} -> false; error:function_clause -> false end.

text(["commands"]) -> text([]);
text(["--help"]) -> text([]);
text(["-h"]) -> text([]);
text([Scope, Alias]) when Alias =:= "commands"; Alias =:= "--help"; Alias =:= "-h" ->
    text([Scope]);
text(["query-syntax"]) ->
    #{progname := Name} = retinue_cli_runtime:context(),
    query_syntax(case Name of "retinue" -> "retinue warframe"; _ -> Name end);
text(["warframe", "query-syntax"]) -> query_syntax("retinue warframe");
text([]) ->
    #{tree := Tree, progname := Name, groups := GroupNames} = retinue_cli_runtime:context(),
    grouped(Name, Tree, GroupNames);
text(Path) ->
    #{tree := Tree, progname := Name} = retinue_cli_runtime:context(),
    Node = retinue_cli_args:node(Path, Tree),
    case maps:find(groups, Node) of
        {ok, Groups} -> grouped(string:join([Name | Path], " "), Node, Groups);
        error -> command_help(Name, Path, Node)
    end.

command_help(Name, Path, Node) ->
    Help = retinue_cli_args:flag(help, "help", "show this help"),
    Display = Node#{help => maps:get(summary, Node, maps:get(help, Node, "")),
                    arguments => [Help#{short => $h} | [display_argument(Arg)
                                  || Arg <- maps:get(arguments, Node, [])]]},
    [argparse:help(Display, #{progname => string:join([Name | Path], " "),
                             columns => min(100, retinue_tty:terminal_width())}),
     maps:get(notes, Node, "")].

grouped(Name, Node, Groups) ->
    Commands = maps:get(commands, Node),
    Rows = [{Group, [{Cmd, maps:get(help, maps:get(Cmd, Commands))} || Cmd <- Names,
        maps:get(help, maps:get(Cmd, Commands)) =/= hidden]} || {Group, Names} <- Groups],
    Width = lists:max([length(Cmd) || {_, Items} <- Rows, {Cmd, _} <- Items]),
    ["USAGE:\n  ", Name, " <command> [options]\n\n",
     [[Group, ":\n", [["  ", string:pad(Cmd, Width), "  ", Summary, "\n"]
                      || {Cmd, Summary} <- Items], "\n"] || {Group, Items} <- Rows],
     "Global options: --json, --utc. Start with '", Name, " doctor' for companion health.\n",
     "Use '", Name, " COMMAND --help' for options.\n"].

query_syntax(Name) ->
    [wf_help_text:query_guide(), "\n", wf_help_text:query_examples(Name)].

display_argument(#{default := []} = Arg) -> display_argument(maps:remove(default, Arg));
display_argument(#{long := "-" ++ Long} = Arg) -> Arg#{name => Long};
display_argument(#{name := query_tokens} = Arg) -> Arg#{name => "QUERY"};
display_argument(Arg) -> Arg.
