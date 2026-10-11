-module(retinue_cli_args).

-export([parse/1, parse/2, parse/3, option/4, flag/3, query/0, format/2,
         node/2, options/1, choices/1, interactive/0]).

parse(Args) -> parse(Args, retinue_cli:command()).

parse(Args, Tree) -> parse(Args, Tree, "retinue").

parse(Args0, Tree, Name) ->
    Args = help_alias(Args0, Tree, Name),
    case argparse:parse(Args, Tree, #{progname => Name}) of
        {ok, Parsed, [_ | Path], Command} ->
            Defaults = inherited_defaults(Path, Tree, #{}),
            {ok, maps:merge(Defaults, Parsed), Path, Command};
        {error, {[_ | Path], undefined, Flag, _}}
          when Flag =:= "--help"; Flag =:= "-h" ->
            {help, Path};
        {error, {[_ | Path], Expected, Actual, _Detail} = Reason} ->
            case Args =/= [] andalso lists:member(lists:last(Args), ["-h", "--help"])
                 andalso not lists:member("--", Args) of
                true -> {help, Path};
                false ->
                    Node = node(Path, Tree),
                    Candidates = maps:keys(maps:get(commands, Node, #{})) ++ options(Node),
                    Hint = case is_list(Actual) andalso Expected =:= undefined of
                               true -> retinue_cli_suggest:suggest(Actual, Candidates);
                               false -> ""
                           end,
                    {error, Path, lists:flatten([argparse:format_error(Reason), Hint]),
                     {Expected, Actual}}
            end
    end.

help_alias(["--help" | Rest], _Tree, _Name) -> ["help" | Rest];
help_alias(["-h" | Rest], _Tree, _Name) -> ["help" | Rest];
help_alias([], _Tree, _Name) -> [];
help_alias(Args, Tree, Name) ->
    case lists:last(Args) =:= "help" andalso not lists:member("--", Args) of
        false -> Args;
        true ->
            Before = lists:droplast(Args),
            case argparse:parse(Before, Tree, #{progname => Name}) of
                {error, {_, Expected, undefined, <<"expected argument">>}}
                  when is_map(Expected) -> Args;
                _ -> Before ++ ["--help"]
            end
    end.

inherited_defaults([], Node, Acc) ->
    maps:merge(Acc, maps:get(defaults, Node, #{}));
inherited_defaults([Name | Rest], Node, Acc) ->
    inherited_defaults(Rest, maps:get(Name, maps:get(commands, Node)),
                       maps:merge(Acc, maps:get(defaults, Node, #{}))).

option(Name, Long, Type, Help) ->
    #{name => Name, long => "-" ++ Long, type => Type, help => Help}.

flag(Name, Long, Help) ->
    #{name => Name, long => "-" ++ Long, action => {store, true}, help => Help}.

query() ->
    [#{name => query_tokens, nargs => list, action => extend, required => false,
       default => [], help => "query expressions"},
     #{name => query_tokens, long => "-", nargs => all, action => extend, help => hidden},
     (option(query_tokens, "search", string, "query expression"))#{
         short => $q, action => append}].

format(Choices0, Default) ->
    Choices = lists:usort([json | Choices0]),
    [(option(output_format, "format", {atom, Choices}, "output format"))#{
         short => $f, default => Default},
     (option(output_format, "output-format", {atom, Choices}, hidden))].

node([], Node) -> Node;
node([Name | Rest], #{commands := Children} = Parent) ->
    Child = maps:get(Name, Children),
    node(Rest, Child#{arguments => maps:get(arguments, Parent, []) ++
                                   maps:get(arguments, Child, []),
                      notes => maps:get(notes, Parent, "") ++ maps:get(notes, Child, "")}).

options(Node) ->
    lists:append([spelling(Arg) || Arg <- maps:get(arguments, Node, [])]).

spelling(Arg) ->
    Short = case maps:find(short, Arg) of {ok, C} -> [[$-, C]]; error -> [] end,
    Long = case maps:find(long, Arg) of
               {ok, "-"} -> [];
               {ok, Name} -> ["-" ++ Name];
               error -> []
           end,
    Short ++ Long.

choices(#{completion := Values}) -> Values;
choices(#{type := {atom, Values}}) -> [atom_to_list(V) || V <- Values];
choices(#{type := {string, [First | _] = Values}}) when is_list(First) -> Values;
choices(_) -> [].

interactive() ->
    case io:getopts() of
        Options when is_list(Options) ->
            proplists:get_value(stdin, Options, false) andalso
                proplists:get_value(stdout, Options, false);
        _ -> false
    end.
