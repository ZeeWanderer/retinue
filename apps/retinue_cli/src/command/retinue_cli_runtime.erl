-module(retinue_cli_runtime).
-export([run/2, with_context/2, context/0, usage/0, fail/1]).

run(Args, Context) -> with_context(Context, fun() -> dispatch(Args) end).

with_context(Context, Run) ->
    Previous = put(?MODULE, Context),
    try Run()
    after
        case Previous of undefined -> erase(?MODULE); _ -> put(?MODULE, Previous) end
    end.

context() ->
    case get(?MODULE) of
        undefined -> retinue_cli:context();
        Context -> Context
    end.

dispatch(Args) ->
    #{tree := Tree, progname := Name} = context(),
    case retinue_cli_args:parse(Args, Tree, Name) of
        {help, Path} -> retinue_help:run(Path);
        {ok, Parsed, _Path, #{handler := Handler}} ->
            retinue_output:with(Parsed, fun() -> invoke(Handler, Parsed) end);
        {error, Path, Message, Detail} ->
            case suggest(Args, Path, Detail) of
                {ok, Corrected} -> dispatch(Corrected);
                none ->
                    io:format(standard_error, "error: ~ts~n", [Message]),
                    io:put_chars(standard_error, retinue_help:text(Path)),
                    halt(2)
            end
    end.

invoke({Module, Function}, Parsed) -> Module:Function(Parsed);
invoke(Function, Parsed) -> Function(Parsed).

usage() -> io:put_chars(retinue_help:text([])).

fail(Message) ->
    io:format(standard_error, "error: ~ts~n", [Message]),
    halt(1).

suggest(Args, Path, {undefined, Unknown}) when is_list(Unknown) ->
    case lists:member("--no-suggest-prompt", lists:takewhile(fun(A) -> A =/= "--" end, Args))
         orelse not retinue_cli_args:interactive() of
        true -> none;
        false ->
            Node = retinue_cli_args:node(Path, maps:get(tree, context())),
            Candidates = maps:keys(maps:get(commands, Node, #{})) ++
                         retinue_cli_args:options(Node),
            suggest_unknown(Args, Unknown, Candidates)
    end;
suggest(_Args, _Path, _Detail) -> none.

suggest_unknown(Args, Unknown, Candidates) ->
    case {length([A || A <- Args, A =:= Unknown]),
          retinue_cli_suggest:suggest_match(Unknown, Candidates)} of
        {1, {ok, Replacement}} when Replacement =/= Unknown ->
            io:format(standard_error, "unknown argument: ~s. use ~s? [enter to accept] ",
                      [Unknown, Replacement]),
            case io:get_line("") of
                Line when is_list(Line) ->
                    case string:lowercase(string:trim(Line)) of
                        Answer when Answer =:= ""; Answer =:= "y"; Answer =:= "yes" ->
                            {ok, [case A of Unknown -> Replacement; _ -> A end || A <- Args]};
                        _ -> none
                    end;
                _ -> none
            end;
        _ -> none
    end.
